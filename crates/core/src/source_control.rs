//! In-memory control for source inspections. No user data or task state is persisted.
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

#[derive(Default)]
struct State {
    claimed: bool,
    cancelled: bool,
    sealed: bool,
    done: bool,
    stage: String,
    attempt: usize,
}
struct Request {
    started: Instant,
    state: Mutex<State>,
}
type Key = (String, String);
static REQUESTS: OnceLock<Mutex<HashMap<Key, Arc<Request>>>> = OnceLock::new();
thread_local! { static CURRENT: RefCell<Option<Arc<Request>>> = const { RefCell::new(None) }; }
const LIMIT: Duration = Duration::from_secs(120);
fn requests() -> &'static Mutex<HashMap<Key, Arc<Request>>> {
    REQUESTS.get_or_init(Default::default)
}
fn scope(data: &Path) -> String {
    crate::protection::path_key(data)
}
fn interrupted(request: &Request, state: &State) -> Result<()> {
    if state.cancelled {
        return Err(crate::errors::coded(
            "SOURCE_CANCELLED",
            json!({}),
            "Source inspection cancelled",
        ));
    }
    if request.started.elapsed() >= LIMIT {
        return Err(crate::errors::coded(
            "SOURCE_TIMEOUT",
            json!({}),
            "Source inspection exceeded 120 seconds",
        ));
    }
    Ok(())
}
pub(crate) fn begin(data: &Path) -> Result<Value> {
    let mut all = requests().lock().unwrap();
    all.retain(|_, r| {
        let s = r.state.lock().unwrap();
        !s.done && (s.claimed || r.started.elapsed() < Duration::from_secs(30))
    });
    let owner = scope(data);
    if all.keys().any(|(s, _)| *s == owner) {
        return Err(crate::errors::coded(
            "SOURCE_BUSY",
            json!({}),
            "A source inspection is still running or cleaning up. Retry shortly.",
        ));
    }
    let id = uuid::Uuid::new_v4().to_string();
    all.insert(
        (owner, id.clone()),
        Arc::new(Request {
            started: Instant::now(),
            state: Mutex::new(State {
                stage: "preparing".into(),
                ..State::default()
            }),
        }),
    );
    Ok(json!({"requestId":id}))
}
pub(crate) fn control(data: &Path, args: &Value, cancel: bool) -> Result<Value> {
    let id = args["requestId"]
        .as_str()
        .context("Missing source request ID")?;
    uuid::Uuid::parse_str(id)?;
    let all = requests().lock().unwrap();
    let Some(request) = all.get(&(scope(data), id.to_owned())) else {
        return Ok(json!({"stage":"finished","elapsedSeconds":0,"attempt":0}));
    };
    let mut state = request.state.lock().unwrap();
    if cancel && !state.sealed {
        state.cancelled = true;
        if !state.claimed {
            state.done = true;
        }
    }
    Ok(
        json!({"stage":if state.done {"finished"} else if state.cancelled {"cancelling"} else {&state.stage},"elapsedSeconds":request.started.elapsed().as_secs(),"attempt":state.attempt}),
    )
}
pub(crate) struct Guard {
    request: Arc<Request>,
}
impl Guard {
    pub(crate) fn enter(data: &Path, args: &Value) -> Result<Self> {
        let id = match args.get("requestId") {
            Some(id) => id.as_str().context("Invalid source request ID")?.to_owned(),
            None => begin(data)?["requestId"].as_str().unwrap().to_owned(),
        };
        let request = requests()
            .lock()
            .unwrap()
            .get(&(scope(data), id))
            .cloned()
            .context("Source request expired; start again")?;
        {
            let mut s = request.state.lock().unwrap();
            interrupted(&request, &s)?;
            if s.claimed || s.done {
                anyhow::bail!("Source request has already been used");
            }
            s.claimed = true;
        }
        CURRENT.with(|c| *c.borrow_mut() = Some(request.clone()));
        Ok(Self { request })
    }
    // Linearize completion against cancellation before handing a snapshot to its caller.
    pub(crate) fn seal(&self) -> Result<()> {
        let mut s = self.request.state.lock().unwrap();
        s.sealed = true;
        interrupted(&self.request, &s)
    }
    pub(crate) fn detach(&self) {
        CURRENT.with(|c| c.borrow_mut().take());
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        self.detach();
        self.request.state.lock().unwrap().done = true;
    }
}
pub(crate) fn checkpoint() -> Result<()> {
    CURRENT.with(|c| match &*c.borrow() {
        Some(r) => interrupted(r, &r.state.lock().unwrap()),
        None => Ok(()),
    })
}
pub(crate) fn stage(name: &str, attempt: usize) -> Result<()> {
    checkpoint()?;
    CURRENT.with(|c| {
        if let Some(r) = &*c.borrow() {
            let mut s = r.state.lock().unwrap();
            s.stage = name.into();
            s.attempt = attempt;
        }
    });
    Ok(())
}
pub(crate) async fn wait<F: std::future::Future>(future: F) -> Result<F::Output> {
    let mut future = std::pin::pin!(future);
    loop {
        checkpoint()?;
        if let Ok(result) = tokio::time::timeout(Duration::from_millis(25), future.as_mut()).await {
            return Ok(result);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancelling_zip_inspection_closes_the_live_proxy_connection() {
        use std::io::Read;
        let fixture = tempfile::tempdir().unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let proxy = format!("http://{}", listener.local_addr().unwrap());
        let (tx, rx) = std::sync::mpsc::channel();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut byte = [0];
            let mut headers = Vec::new();
            while !headers.ends_with(b"\r\n\r\n") {
                socket.read_exact(&mut byte).unwrap();
                headers.push(byte[0]);
            }
            assert!(String::from_utf8_lossy(&headers).starts_with("CONNECT example.test:443"));
            tx.send(()).unwrap();
            // A cancelled client must close the connection without a proxy response.
            socket.read(&mut byte).unwrap()
        });
        let data = fixture.path().join("data");
        crate::dispatch(&data, "network.save", json!({"proxyUrl":proxy})).unwrap();
        let request = begin(&data).unwrap();
        let worker_data = data.clone();
        let args = json!({"requestId":request["requestId"],"source":{"kind":"zip","locator":"https://example.test/fixture.zip"}});
        let worker =
            std::thread::spawn(move || crate::dispatch(&worker_data, "sources.inspect", args));
        rx.recv_timeout(Duration::from_secs(10)).unwrap();
        control(&data, &request, true).unwrap();
        assert_eq!(
            crate::errors::CoreError::from_error(worker.join().unwrap().unwrap_err()).code,
            "SOURCE_CANCELLED"
        );
        assert_eq!(server.join().unwrap(), 0);
        assert_eq!(
            std::fs::read_dir(data.join("source-inspections"))
                .unwrap()
                .count(),
            0
        );
    }
    #[test]
    fn cancelled_inspection_removes_snapshot_and_preserves_source() {
        let fixture = tempfile::tempdir().unwrap();
        let source = fixture.path().join("source");
        let data = fixture.path().join("data");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("SKILL.md"), "# Fictional Skill").unwrap();
        // Many small files keep the copy observable without real network or Agent data.
        for i in 0..2000 {
            std::fs::write(source.join(format!("file-{i}.txt")), "fixture").unwrap();
        }
        let request = begin(&data).unwrap();
        let worker_data = data.clone();
        let args =
            json!({"requestId":request["requestId"],"source":{"kind":"local","locator":source}});
        let worker =
            std::thread::spawn(move || crate::dispatch(&worker_data, "sources.inspect", args));
        let deadline = Instant::now() + Duration::from_secs(10);
        // Wait for an actual staged file before cancelling, exercising cleanup of a partial copy.
        loop {
            let staged = walkdir::WalkDir::new(data.join("source-inspections"))
                .into_iter()
                .filter_map(Result::ok)
                .any(|e| e.file_type().is_file());
            if staged {
                break;
            }
            assert!(
                !worker.is_finished() && Instant::now() < deadline,
                "copy was not observed"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        control(&data, &request, true).unwrap();
        let error = worker.join().unwrap().unwrap_err();
        assert_eq!(
            crate::errors::CoreError::from_error(error).code,
            "SOURCE_CANCELLED"
        );
        assert_eq!(
            std::fs::read_dir(data.join("source-inspections"))
                .unwrap()
                .count(),
            0
        );
        let store = crate::store::Store::open(&data).unwrap();
        let count: i64 = store
            .conn
            .query_row("SELECT count(*) FROM source_inspections", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
        assert_eq!(std::fs::read_dir(&source).unwrap().count(), 2001);
        assert_eq!(
            std::fs::read_to_string(source.join("SKILL.md")).unwrap(),
            "# Fictional Skill"
        );
        let next = begin(&data).unwrap();
        control(&data, &next, true).unwrap();
    }
    #[test]
    fn cancellation_is_scoped_single_use_and_releases_slot_after_cleanup() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let request = begin(a.path()).unwrap();
        assert!(begin(a.path()).is_err());
        let guard = Guard::enter(a.path(), &request).unwrap();
        assert!(Guard::enter(a.path(), &request).is_err());
        control(b.path(), &request, true).unwrap();
        assert!(checkpoint().is_ok());
        control(a.path(), &request, true).unwrap();
        assert!(checkpoint().is_err());
        assert!(guard.seal().is_err());
        guard.detach();
        assert!(checkpoint().is_ok());
        assert!(begin(a.path()).is_err());
        drop(guard);
        assert!(begin(a.path()).is_ok());
    }
    #[test]
    fn cancellation_before_claim_prevents_work_and_completion_wins_late_cancel() {
        let a = tempfile::tempdir().unwrap();
        let request = begin(a.path()).unwrap();
        control(a.path(), &request, true).unwrap();
        assert!(Guard::enter(a.path(), &request).is_err());
        let next = begin(a.path()).unwrap();
        let guard = Guard::enter(a.path(), &next).unwrap();
        guard.seal().unwrap();
        control(a.path(), &next, true).unwrap();
        assert!(checkpoint().is_ok());
    }
    #[test]
    fn cancellation_stops_a_process_before_returning() {
        let dir = tempfile::tempdir().unwrap();
        let request = begin(dir.path()).unwrap();
        let data = dir.path().to_path_buf();
        let args = request.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let _guard = Guard::enter(&data, &args).unwrap();
            tx.send(()).unwrap();
            #[cfg(windows)]
            let (program, args) = (
                "powershell.exe",
                vec![
                    "-NoProfile".into(),
                    "-NonInteractive".into(),
                    "-Command".into(),
                    "Start-Sleep -Seconds 60".into(),
                ],
            );
            #[cfg(not(windows))]
            let (program, args) = ("sh", vec!["-c".into(), "exec sleep 60".into()]);
            crate::adapters::run_command(program, &args, None, Duration::from_secs(60))
        });
        rx.recv_timeout(Duration::from_secs(5)).unwrap();
        std::thread::sleep(Duration::from_millis(150));
        let start = Instant::now();
        control(dir.path(), &request, true).unwrap();
        let error = worker.join().unwrap().unwrap_err();
        assert_eq!(
            crate::errors::CoreError::from_error(error).code,
            "SOURCE_CANCELLED"
        );
        assert!(start.elapsed() < Duration::from_secs(8));
        assert!(begin(dir.path()).is_ok());
    }
    #[test]
    fn cancelled_network_future_is_dropped_and_deadline_is_shared() {
        struct Pending(std::sync::Arc<std::sync::atomic::AtomicBool>);
        impl std::future::Future for Pending {
            type Output = ();
            fn poll(
                self: std::pin::Pin<&mut Self>,
                _: &mut std::task::Context<'_>,
            ) -> std::task::Poll<()> {
                std::task::Poll::Pending
            }
        }
        impl Drop for Pending {
            fn drop(&mut self) {
                self.0.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let args = begin(dir.path()).unwrap();
        let guard = Guard::enter(dir.path(), &args).unwrap();
        let flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        control(dir.path(), &args, true).unwrap();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        assert!(rt.block_on(wait(Pending(flag.clone()))).is_err());
        assert!(flag.load(std::sync::atomic::Ordering::SeqCst));
        drop(guard);
        let expired = Request {
            started: Instant::now() - LIMIT,
            state: Mutex::new(State::default()),
        };
        assert_eq!(
            crate::errors::CoreError::from_error(
                interrupted(&expired, &State::default()).unwrap_err()
            )
            .code,
            "SOURCE_TIMEOUT"
        );
    }
}
