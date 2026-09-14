import { useEffect, useRef, useState } from 'react';
import { X } from 'lucide-react';
import * as api from './api';
import { parseSkillSourceInput } from './skillSourceInput';
import { skillPresent } from './SkillAgentIcons';
import { useAgentNames } from './useAgentNames';
import { SkillCandidateDetails } from './SkillCandidateDetails';
import { Badge, Button } from './ui';
import { useT } from './i18n';
import type { Skill, SkillCandidate, SkillDeployment, Source } from './types';

import type { Repository } from './contracts';
export type { Repository } from './contracts';
type Props = { skills: Skill[]; deployments: SkillDeployment[]; refresh: () => Promise<void>; onDelete: (skill: Skill) => void; onClose: () => void };
export function RepositoryManager({ skills, deployments, refresh, onDelete, onClose }: Props) {
  const t = useT();
  const [repositories, setRepositories] = useState<Repository[]>([]);
  const [locator, setLocator] = useState('');
  const [browsing, setBrowsing] = useState<{ repositoryId: string; candidates: SkillCandidate[] }>();
  const [added, setAdded] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const inspection = useRef<string>();
  const mounted = useRef(true);
  const agentName = useAgentNames();
  const release = () => { const id = inspection.current; inspection.current = undefined; if (id) void api.dispatch('sources.release', { inspectionId: id }).catch(() => {}); };
  useEffect(() => { mounted.current = true; api.dispatch('repositories.list').then(r => { if (mounted.current) setRepositories(r.repositories || []); }).catch(e => { if (mounted.current) setError(String(e)); }); return () => { mounted.current = false; release(); }; }, []);
  const collapse = () => { release(); setBrowsing(undefined); setAdded({}); setError(''); };
  const save = async () => {
    setBusy(true); setError(''); let inspectionId: string | undefined;
    try {
      const { source } = parseSkillSourceInput(locator, 'git', undefined, t);
      const inspected = await api.dispatch('sources.inspect', { source });
      inspectionId = inspected.inspectionId;
      const result = await api.dispatch('repositories.save', { inspectionId });
      setRepositories(result.repositories); setLocator('');
    } catch (e) { setError(String(e)); }
    finally { if (inspectionId) await api.dispatch('sources.release', { inspectionId }).catch(() => {}); setBusy(false); }
  };
  const remove = async (id: string) => {
    setBusy(true); setError('');
    try { const r = await api.dispatch('repositories.remove', { id }); setRepositories(r.repositories); if (browsing?.repositoryId === id) collapse(); }
    catch (e) { setError(String(e)); } finally { setBusy(false); }
  };
  // Reading a repository never writes anywhere; the snapshot is released on collapse or close.
  const browse = async (repo: Repository) => {
    collapse(); setBusy(true);
    try {
      const result = await api.dispatch('sources.inspect', { source: repo.source });
      if (!mounted.current) { await api.dispatch('sources.release', { inspectionId: result.inspectionId }); return; }
      inspection.current = result.inspectionId; setBrowsing({ repositoryId: repo.id, candidates: result.candidates || [] });
      if (!(result.candidates || []).length) setError(t('repos.noSkillMd'));
    } catch (e) { if (mounted.current) setError(String(e)); }
    finally { if (mounted.current) setBusy(false); }
  };
  // The reported membership is a point-in-time answer; confirm the Skill is still in the library.
  const inLibrary = (candidate: SkillCandidate) => { const id = added[candidate.subpath] || candidate.skillId; return id && skills.some(s => s.id === id) ? id : undefined; };
  const add = async (candidate: SkillCandidate) => {
    const inspectionId = inspection.current; if (!inspectionId) return;
    setBusy(true); setError('');
    try { const result = await api.dispatch('skills.add', { inspectionId, subpath: candidate.subpath }); setAdded(v => ({ ...v, [candidate.subpath]: result.skill.id })); await refresh(); }
    catch (e) { if (mounted.current) setError(String(e)); } finally { if (mounted.current) setBusy(false); }
  };
  const installedIn = (skillId: string) => [...new Set(deployments.filter(d => d.skillId === skillId && skillPresent(d)).map(d => agentName(d.agent)))].sort(t.compare);
  return <div className="modal-backdrop"><section className="modal modal--wide" role="dialog" aria-modal="true" aria-labelledby="repositories-title">
    <header className="modal__header"><h2 id="repositories-title">{t('repos.title')}</h2><button className="icon-button" aria-label={t('repos.close')} disabled={busy} onClick={onClose}><X size={18}/></button></header>
    <div className="modal__body">
      <label className="field"><span>{t('repos.address')}</span><div className="add-skill__source"><input value={locator} disabled={busy} onChange={e => setLocator(e.target.value)} placeholder={t('repos.addressPlaceholder')}/><button className="button button--primary" disabled={busy || !locator.trim()} onClick={() => void save()}>{t('repos.add')}</button></div></label>
      <div className="library-list">{repositories.map(repo => { const candidates = browsing && browsing.repositoryId === repo.id ? browsing.candidates : undefined; const name = repo.source.locator.split('/').slice(-2).join('/'); return <article className="repository-row" key={repo.id}>
        <div className="library-row__main"><h3>{name} {repo.derived&&<Badge tone="neutral">{t('repos.linkedSource')}</Badge>}</h3><p className="library-path">{repo.source.locator}{repo.source.revision ? ' · ' + repo.source.revision : ''}</p>
          {candidates && <div className="add-skill__candidates">{candidates.map(candidate => { const skillId = inLibrary(candidate); const installed = skillId ? installedIn(skillId) : []; return <div className="import-skills__item repository-candidate" key={candidate.subpath}>
            <span><strong>{candidate.name}</strong><small>{candidate.description}</small><SkillCandidateDetails candidate={candidate}/>{skillId&&<small>{installed.length ? t('repos.installedTo', { agents: installed.join(t.lang === 'zh' ? '、' : ', ') }) : t('repos.notInstalled')}</small>}</span>
            <Badge tone={skillId ? 'green' : 'neutral'}>{skillId ? t('repos.inLibrary') : t('repos.notInLibrary')}</Badge>
            {skillId ? <Button size="sm" disabled={busy} aria-label={t('repos.deleteLabel', { name: candidate.name })} onClick={() => { const skill = skills.find(s => s.id === skillId); if (skill) onDelete(skill); }}>{t('repos.deleteFromLibrary')}</Button> : <Button size="sm" variant="primary" disabled={busy} aria-label={t('repos.addLabel', { name: candidate.name })} onClick={() => void add(candidate)}>{t('repos.addToLibrary')}</Button>}
          </div>; })}</div>}
        </div>
        <div className="library-actions"><Button disabled={busy} onClick={() => candidates ? collapse() : void browse(repo)}>{candidates ? t('repos.collapse') : t('repos.browse')}</Button><Button disabled={busy} aria-label={t('repos.removeLabel', { locator: repo.source.locator })} onClick={() => void remove(repo.id)}>{t('repos.remove')}</Button></div>
      </article>; })}</div>
      {!repositories.length && <p className="form-help">{t('repos.empty')}</p>}

      {error && <p role="alert" className="text-error">{t.backend(error)}</p>}
    </div>
  </section></div>;
}
