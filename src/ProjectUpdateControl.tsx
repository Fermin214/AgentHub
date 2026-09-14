import { checkError } from './checkError';
import { useEffect, useState } from 'react';
import { RefreshCw } from 'lucide-react';
import * as api from './api';
import { useT } from './i18n';
import type { LocalProject, UpdateCheck } from './types';

const STATUSES = ['available', 'current', 'failed', 'diverged', 'unsupported'];

export function ProjectUpdateControl({ project, update, onResult, onOpen }: { project: LocalProject; update?: UpdateCheck; onResult?: (u: UpdateCheck) => void; onOpen: () => void }) {
  const t = useT();
  const [cached, setCached] = useState<UpdateCheck>();
  const [revision, setRevision] = useState(0);
  const [busy, setBusy] = useState(false); const [error, setError] = useState('');
  useEffect(() => {
    let alive = true;
    api.dispatch('projects.status', { projectId: project.id }).then(r => {
      if (alive) { setError(''); setCached(r.update || undefined); }
    }).catch(e => { if (alive) setError(String(e)); });
    return () => { alive = false; };
  }, [project.id, project.updatedAt, update?.checkedAt, revision]);
  useEffect(() => { const refresh = () => setRevision(r => r + 1); window.addEventListener('agenthub-project-updated', refresh); return () => window.removeEventListener('agenthub-project-updated', refresh); }, []);
  const label = (status: string) => t(STATUSES.includes(status) ? 'upstream.' + status : 'upstream.unknown');
  const check = async () => {
    setBusy(true); setError('');
    try { const r = await api.dispatch('projects.check', { projectId: project.id }); setCached(r.update); onResult?.(r.update); }
    catch (e) { setError(String(e)); } finally { setBusy(false); }
  };
  const hint = cached?.message ? (cached.status==='failed'?checkError(cached.message,t):t.backend(cached.message)) : undefined;
  if (!project.gitTrusted) return <p>{t('projects.gitUntrusted')}</p>;
  return <div className="project-update-control"><span title={hint}>{error ? t('upstream.failed') : cached ? label(cached.status) : t('upstream.notChecked')}</span>
    <time title={hint} dateTime={cached?.checkedAt}>{cached?.checkedAt ? t.dateTime(cached.checkedAt) : null}</time>
    <div><button className="button button--quiet" disabled={busy} aria-label={t('upstream.checkLabel', { name: project.name })} onClick={() => void check()}><RefreshCw size={13} className={busy ? 'spin' : ''}/>{busy ? t('upstream.checking') : t('upstream.check')}</button><button className="button button--quiet" aria-label={t('upstream.detailsLabel', { name: project.name })} onClick={onOpen}>{t('upstream.details')}</button></div>
    {error && <span role="alert" className="text-error">{checkError(error,t)}</span>}
  </div>;
}
