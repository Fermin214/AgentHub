export async function installFaults(expectedDataDir) {
  if (window.__acceptanceFaults) throw new Error('Run __acceptanceFaults.restore() before installing again.');
  const internals = window.__TAURI_INTERNALS__;
  if (!internals?.invoke) throw new Error('A native Tauri window is required.');
  const snapshot = await internals.invoke('dispatch', { method: 'snapshot', args: {} });
  const normalize = value => String(value).replaceAll('\\', '/').toLowerCase();
  const dataDir = normalize(snapshot.dataDir || '');
  if (dataDir !== normalize(expectedDataDir)) {
    throw new Error('Refusing fault injection outside this run DataDir.');
  }
  // Native Tauri's invoke/ipc properties are readonly. Windows custom-protocol
  // IPC uses fetch; return an error response instead of rejecting fetch, because
  // a rejected fetch causes Tauri to retry via its postMessage transport.
  const originalFetch = window.fetch;
  const dispatchUrl = internals.convertFileSrc('dispatch', 'ipc');
  const qa = { delayMs: 0, delayAfterMs: 0, failSaveOnce: false, failRefreshOnce: false, failRefresh: false, calls: [], active: 0, observed: 0 };
  const wait = milliseconds => new Promise(resolve => setTimeout(resolve, milliseconds));
  const rejected = message => new Response(JSON.stringify(message), {
    headers: { 'Content-Type': 'application/json', 'Tauri-Response': 'error' }
  });
  window.fetch = async function(input, options) {
    if (String(input) !== dispatchUrl || typeof options?.body !== 'string') return originalFetch.call(this, input, options);
    let payload;
    try { payload = JSON.parse(options.body); } catch { return originalFetch.call(this, input, options); }
    qa.observed++;
    const method = payload?.method;
    if (method === 'snapshot' && (qa.failRefresh || qa.failRefreshOnce)) {
      qa.failRefreshOnce = false;
      return rejected('ACCEPTANCE_INJECTED: snapshot refresh failed');
    }
    const favorite = method === 'skills.metadata.save' && typeof payload.args?.favorite === 'boolean';
    if (!favorite) return originalFetch.call(this, input, options);
    qa.calls.push({ ids: [...payload.args.ids], favorite: payload.args.favorite });
    qa.active++;
    try {
      if (qa.delayMs) await wait(qa.delayMs);
      if (qa.failSaveOnce) {
        qa.failSaveOnce = false;
        return rejected('ACCEPTANCE_INJECTED: favorite save rejected before writing');
      }
      const result = await originalFetch.call(this, input, options);
      if (qa.delayAfterMs) await wait(qa.delayAfterMs);
      return result;
    } finally { qa.active--; }
  };
  qa.restore = () => {
    if (qa.active) throw new Error('Wait for pending injected requests before restoring.');
    window.fetch = originalFetch;
    delete window.__acceptanceFaults;
  };
  window.__acceptanceFaults = qa;
  try {
    await internals.invoke('dispatch', { method: 'snapshot', args: {} });
    if (!qa.observed) throw new Error('This window is not using fetch IPC. Reopen it or mark injection not-run.');
  } catch (error) { qa.restore(); throw error; }
  console.log('Acceptance IPC injection ready; use __acceptanceFaults.restore() when finished.', dataDir);
}
