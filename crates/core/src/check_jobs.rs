//! Bounded checks and per-batch source reuse. Each completed Skill keeps its
//! own immutable source copy, so releasing a batch cannot invalidate an update.
use crate::{native, protection, safe_files, sources, store::Store};
use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex, OnceLock},
};

const MAX_CHECKS: usize = 3;
type Key = (String, String);
type Outcome = std::result::Result<Value, String>;
#[derive(Default)]
struct Flight {
    result: Mutex<Option<Outcome>>,
    ready: Condvar,
}
#[derive(Default)]
struct State {
    running: HashMap<Key, Arc<Flight>>,
    active: HashMap<String, usize>,
    batches: HashMap<Key, Arc<Batch>>,
}
#[derive(Default)]
struct Jobs {
    state: Mutex<State>,
    available: Condvar,
}
static JOBS: OnceLock<Jobs> = OnceLock::new();
fn jobs() -> &'static Jobs {
    JOBS.get_or_init(Jobs::default)
}
fn scope(store: &Store) -> String {
    protection::path_key(store.data_dir())
}

struct Guard {
    key: Key,
    flight: Arc<Flight>,
}
impl Drop for Guard {
    fn drop(&mut self) {
        let mut result = self.flight.result.lock().unwrap();
        if result.is_none() {
            *result = Some(Err("Skill 检查意外中断，请重试".into()));
        }
        self.flight.ready.notify_all();
        drop(result);
        let mut state = jobs().state.lock().unwrap();
        state.running.remove(&self.key);
        if let Some(n) = state.active.get_mut(&self.key.0) {
            *n -= 1;
        }
        jobs().available.notify_all();
    }
}
pub(crate) fn run(store: &Store, id: &str, work: impl FnOnce() -> Result<Value>) -> Result<Value> {
    let key = (scope(store), id.to_owned());
    let mut state = jobs().state.lock().unwrap();
    if let Some(flight) = state.running.get(&key).cloned() {
        drop(state);
        let mut result = flight.result.lock().unwrap();
        while result.is_none() {
            result = flight.ready.wait(result).unwrap();
        }
        return result.as_ref().unwrap().clone().map_err(|e| anyhow!(e));
    }
    // Register before waiting for a slot, so duplicate requests share queued work.
    let flight = Arc::new(Flight::default());
    state.running.insert(key.clone(), flight.clone());
    while state.active.get(&key.0).copied().unwrap_or(0) >= MAX_CHECKS {
        state = jobs().available.wait(state).unwrap();
    }
    *state.active.entry(key.0.clone()).or_default() += 1;
    drop(state);
    let _guard = Guard {
        key,
        flight: flight.clone(),
    };
    let result = work().map_err(|e| format!("{e:#}"));
    *flight.result.lock().unwrap() = Some(result.clone());
    flight.ready.notify_all();
    result.map_err(|e| anyhow!(e))
}

struct Acquired {
    root: PathBuf,
    inspection: Value,
}
type SourceCell = OnceLock<std::result::Result<Arc<Acquired>, String>>;
pub(crate) struct Batch {
    root: PathBuf,
    sources: Mutex<HashMap<String, Arc<SourceCell>>>,
}
impl Drop for Batch {
    fn drop(&mut self) {
        let _ = safe_files::remove_tree(&self.root);
    }
}
pub(crate) fn begin(store: &Store) -> Result<Value> {
    let id = uuid::Uuid::new_v4().to_string();
    let root = store.data_dir().join("skill-check-batches").join(&id);
    safe_files::safe_directory(&root)?;
    jobs().state.lock().unwrap().batches.insert(
        (scope(store), id.clone()),
        Arc::new(Batch {
            root,
            sources: Mutex::new(HashMap::new()),
        }),
    );
    Ok(json!({"batchId":id,"concurrency":MAX_CHECKS}))
}
pub(crate) fn end(store: &Store, id: &str) -> Result<Value> {
    uuid::Uuid::parse_str(id)?;
    let batch = jobs()
        .state
        .lock()
        .unwrap()
        .batches
        .remove(&(scope(store), id.to_owned()));
    drop(batch);
    Ok(json!({"ok":true}))
}
pub(crate) fn batch(store: &Store, id: &str) -> Result<Arc<Batch>> {
    uuid::Uuid::parse_str(id)?;
    jobs()
        .state
        .lock()
        .unwrap()
        .batches
        .get(&(scope(store), id.to_owned()))
        .cloned()
        .context("批量检查已结束，请重新检查")
}
impl Batch {
    pub(crate) fn stage(&self, source: &Value, proxy: &str, destination: &Path) -> Result<PathBuf> {
        let normalized = native::normalize_source(source)?;
        let mut source: crate::model::Source = serde_json::from_value(normalized.clone())?;
        let selected = source.subpath.take();
        let key = serde_json::to_string(
            &json!({"source":sources::source_identity(&source)?,"proxy":proxy}),
        )?;
        let cell = self
            .sources
            .lock()
            .unwrap()
            .entry(key)
            .or_insert_with(|| Arc::new(OnceLock::new()))
            .clone();
        let acquired = cell
            .get_or_init(|| {
                let root = self.root.join(uuid::Uuid::new_v4().to_string());
                native::inspect_source_with_proxy(&json!(source), &root, Some(proxy))
                    .map(|inspection| Arc::new(Acquired { root, inspection }))
                    .map_err(|e| format!("{e:#}"))
            })
            .clone()
            .map_err(|e| anyhow!(e))?;
        let payload = acquired.root.join("source");
        let selected = if let Some(subpath) = selected.filter(|p| !p.is_empty()) {
            payload.join(subpath)
        } else if payload.join("SKILL.md").is_file() {
            payload
        } else {
            let candidates = acquired.inspection["candidates"]
                .as_array()
                .context("来源候选无效")?;
            if candidates.len() != 1 {
                return Err(anyhow!("来源包含多个 Skill，请设置具体子目录"));
            }
            payload.join(candidates[0]["subpath"].as_str().context("来源路径缺失")?)
        };
        if !selected.join("SKILL.md").is_file() {
            return Err(anyhow!("来源子目录没有 SKILL.md"));
        }
        let path = destination.join("source");
        safe_files::copy_tree(&selected, &path)?;
        Ok(path)
    }
}
/// Only abandoned process snapshots can reach this age; active batches are also
/// explicitly excluded. No persistent cache crosses a user-started check round.
pub(crate) fn cleanup(store: &Store) -> Result<()> {
    let root = store.data_dir().join("skill-check-batches");
    if !root.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let id = entry.file_name().to_string_lossy().to_string();
        if uuid::Uuid::parse_str(&id).is_err() {
            continue;
        }
        let active = jobs()
            .state
            .lock()
            .unwrap()
            .batches
            .contains_key(&(scope(store), id));
        if !active
            && entry
                .metadata()?
                .modified()?
                .elapsed()
                .unwrap_or_default()
                .as_secs()
                > 86400
        {
            safe_files::remove_tree(&entry.path())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc,
    };
    #[test]
    fn independent_checks_overlap_but_never_exceed_three_slots() {
        let t = tempfile::tempdir().unwrap();
        let data = t.path().to_path_buf();
        let live = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        let (tx, rx) = mpsc::channel();
        std::thread::scope(|scope| {
            for n in 0..8 {
                let gate = gate.clone();
                let tx = tx.clone();
                let data = &data;
                let live = &live;
                let peak = &peak;
                scope.spawn(move || {
                    let store = Store::open(data).unwrap();
                    run(&store, &n.to_string(), || {
                        let count = live.fetch_add(1, Ordering::SeqCst) + 1;
                        peak.fetch_max(count, Ordering::SeqCst);
                        tx.send(()).unwrap();
                        let (lock, ready) = &*gate;
                        let mut open = lock.lock().unwrap();
                        while !*open {
                            open = ready.wait(open).unwrap();
                        }
                        live.fetch_sub(1, Ordering::SeqCst);
                        Ok(json!({"ok":true}))
                    })
                    .unwrap();
                });
            }
            for _ in 0..3 {
                rx.recv_timeout(std::time::Duration::from_secs(10)).unwrap();
            }
            assert_eq!(live.load(Ordering::SeqCst), 3);
            assert!(rx.try_recv().is_err());
            let (lock, ready) = &*gate;
            *lock.lock().unwrap() = true;
            ready.notify_all();
        });
        assert_eq!(peak.load(Ordering::SeqCst), 3);
        assert_eq!(live.load(Ordering::SeqCst), 0);
    }
}
