import { useEffect, useState } from 'react';
import { GitBranch, GitCommitHorizontal, LoaderCircle } from 'lucide-react';
import * as api from './api';
import { checkError } from './checkError';
import { displayPath } from './displayPath';
import { useT } from './i18n';
import { Button, Modal } from './ui';
import type { LocalProject } from './types';
import type { ProjectState, ProjectCheck } from './contracts';

type Props = { onManageSkills?: () => void; project: LocalProject; onClose: () => void };

export function ProjectUpdateDialog({ project, onClose, onManageSkills }: Props) {
  const t = useT();
  const [state, setState] = useState<ProjectState>();
  const [lastCheck, setLastCheck] = useState<ProjectCheck>();
  const [busy, setBusy] = useState(true);
  const [error, setError] = useState('');
  const [commits, setCommits] = useState<string[]>([]);
  useEffect(() => {
    let active = true;
    api.dispatch('projects.inspect', { projectId: project.id }).then(result => {
      if (!active) return;
      setState(result.state);
      setCommits((result.state.upstreamCommits || '').split('\n').filter(Boolean));
      setLastCheck(result.lastCheck || undefined);
    }).catch(cause => { if (active) setError(String(cause)); }).finally(() => { if (active) setBusy(false); });
    return () => { active = false; };
  }, [project.id]);
  const loadMore = async () => {
    if (!state?.head || !state.upstreamHead) return;
    setBusy(true); setError('');
    try {
      const page = await api.dispatch('projects.commits', { projectId: project.id, head: state.head, upstreamHead: state.upstreamHead, offset: commits.length });
      setCommits(value => [...value, ...page.commits.split('\n').filter(Boolean)]);
    } catch (cause) { setError(String(cause)); } finally { setBusy(false); }
  };
  const fetchedAt = lastCheck?.fetchedAt || (lastCheck?.status !== 'failed' ? lastCheck?.checkedAt : undefined);
  const commitRow = (line: string, index: number) => {
    const match = line.match(/^(\S+)\s+(.*)$/);
    return <li key={index}><GitCommitHorizontal size={16}/><code>{match?.[1] || ''}</code><span>{match?.[2] || line}</span></li>;
  };
  return <Modal title={t('upstream.dialogTitle', { name: project.name })} wide onClose={onClose} footer={<Button variant="secondary" onClick={onClose}>{t('common.close')}</Button>}>
    <div className="project-update">
      <div className="project-identity"><div>{state && <span className="branch-label"><GitBranch size={16}/>{state.branch || t(state.isGit ? 'upstream.detachedHead' : 'upstream.plainFolder')}</span>}<p className="library-path">{displayPath(project.path)}</p></div>{onManageSkills && <Button onClick={onManageSkills}>{t('upstream.manageSkills')}</Button>}</div>
      {busy && !state && <p role="status"><LoaderCircle size={15} className="spin"/> {t('upstream.loadingDetails')}</p>}
      {state?.isGit && <section className="upstream-summary"><strong className="upstream-leading">{t('upstream.newCommits', { n: state.behind ?? 0 })}</strong><div className="upstream-context"><p>{t('upstream.localOnly', { n: state.ahead ?? 0 })}</p><p className="library-path">{state.upstream || t('upstream.noUpstream')}</p>{state.remoteUrl && <p className="library-path">{state.remoteUrl}</p>}</div></section>}
      {state && <div className="check-status"><p>{t.backend(state?.message || '') || t('upstream.notCheckedYet')}</p>{state?.isGit && <p>{t('upstream.localRecord')}</p>}{fetchedAt && <time>{t('upstream.fetchedAt', { time: t.dateTime(fetchedAt) })}</time>}{lastCheck?.status === 'failed' && <><p className="text-error">{checkError(lastCheck.message, t)}</p>{lastCheck.checkedAt && <time>{t('upstream.failedAt', { time: t.dateTime(lastCheck.checkedAt) })}</time>}<details className="check-diagnostic"><summary>{t('common.errorDetails')}</summary><pre>{lastCheck.message}</pre></details></>}</div>}
      {(state?.commonAncestor || commits.length > 0) && <section className="upstream-section"><h3>{t('upstream.commitsTitle')} <span>{t('upstream.commitRange', { n: state?.commitRangeTotal ?? commits.length })}</span></h3><ol className="commit-list">{commits.map(commitRow)}</ol>{commits.length < (state?.commitRangeTotal || 0) && <Button disabled={busy} onClick={() => void loadMore()}>{t('upstream.showMore', { shown: commits.length, total: state?.commitRangeTotal ?? commits.length })}</Button>}{state?.commonAncestor && <div className="commit-ancestor"><GitCommitHorizontal size={16}/><code title={state.commonAncestor}>{state.commonAncestorSummary?.split(' ')[0] || state.commonAncestor.slice(0, 12)}</code><span>{state.commonAncestorSummary?.split(' ').slice(1).join(' ')} <strong>{t('upstream.mergeBase')}</strong></span></div>}</section>}
      {state?.upstreamChanges && <details className="upstream-section"><summary>{t('upstream.fileChanges')}</summary><pre className="project-update__output">{state.upstreamChanges}</pre></details>}
      {state?.changes && <details className="upstream-section"><summary>{t('upstream.localChanges')} <span className="badge badge--amber">{t('upstream.fileCount', { n: state.changes.trim().split('\n').length })}</span></summary><pre className="project-update__output">{state.changes}</pre></details>}
      {error && <p role="alert" className="text-error">{t.backend(error)}</p>}
    </div>
  </Modal>;
}
