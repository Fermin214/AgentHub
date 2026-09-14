import { useEffect, useId, useState } from 'react';
import * as api from './api';
import { Badge, Modal, SearchField } from './ui';
import { displayPath } from './displayPath';
import { useT } from './i18n';
import type { LocalProject } from './types';

import type { RepositoryBookmark } from './contracts';
export type { RepositoryBookmark } from './contracts';
const STATUSES = ['interested', 'in_use', 'uninstalled'] as const;
const blank = (): RepositoryBookmark => ({ id: '', name: '', url: '', notes: '', status: 'interested', archived: false });
export function RepositoryBookmarks({ projects, dataScope, archived = false, addRequest=0, refresh, onEditProject, onDeleteProject }: { projects: LocalProject[]; dataScope: string; archived?: boolean; addRequest?:number; refresh?:()=>Promise<void>; onEditProject?:(p:LocalProject)=>void; onDeleteProject?:(p:LocalProject)=>void }) {
  const t = useT();
  const formId = useId();
  const label = (status: RepositoryBookmark['status']) => t('bookmarks.status.' + status);
  useEffect(()=>{if(addRequest>0){setTagDraft('');setEditing(blank());setError('');}},[addRequest]);
  const [items, setItems] = useState<RepositoryBookmark[]>([]);
  const [editing, setEditing] = useState<RepositoryBookmark>();
  const [query, setQuery] = useState('');
  const [status, setStatus] = useState('all');
  const [sort, setSort] = useState('created');
  const [tag, setTag] = useState('');
  const [tagDraft, setTagDraft] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const load = async () => { const result = await api.dispatch('bookmarks.list'); setItems(result.items); };
  const projectKey=projects.map(p=>[p.id,p.path,p.archived].join(':')).join('|');
  useEffect(() => { let alive = true; api.dispatch('bookmarks.sync').then(r => { if (alive) setItems(r.items||[]); }).catch(e => { if (alive) setError(String(e)); }); return () => { alive = false; }; }, [dataScope,projectKey]);
  const mutate = async (work: () => Promise<unknown>) => { setBusy(true); setError(''); try { await work(); await load(); await refresh?.(); setEditing(undefined); } catch (e) { setError(String(e)); } finally { setBusy(false); } };
  const visible = items.filter(item => Boolean(item.archived) === archived && (status === 'all' || item.status === status) && (!tag || item.tags?.includes(tag)) && [item.name, item.url, item.notes, projects.find(p=>p.id===item.projectId)?.name||'', projects.find(p=>p.id===item.projectId)?.path||'', ...(item.tags || [])].some(s => s.toLowerCase().includes(query.trim().toLowerCase())));
  const rows:Array<{item?:RepositoryBookmark;project?:LocalProject;related:RepositoryBookmark[]}>=visible.filter(item=>!archived||!item.projectId||visible.find(i=>i.projectId===item.projectId)?.id===item.id).map(item=>({item,project:projects.find(p=>p.id===item.projectId),related:archived&&item.projectId?visible.filter(i=>i.projectId===item.projectId&&i.id!==item.id):[]}));
  if(archived&&status==='all'&&!tag)for(const project of projects.filter(p=>p.archived&&!items.some(i=>i.projectId===p.id)&&[p.name,p.path].join(' ').toLowerCase().includes(query.trim().toLowerCase())))rows.push({project,related:[]});
  rows.sort((a,b)=>{const left=a.item||a.project!;const right=b.item||b.project!;return (sort==='name'?0:(sort==='updated'?right.updatedAt||right.createdAt||'':right.createdAt||'').localeCompare(sort==='updated'?left.updatedAt||left.createdAt||'':left.createdAt||''))||t.compare(left.name,right.name);});
  return <section className="bookmark-section" aria-label={t('bookmarks.section')}>
    <div className="toolbar"><SearchField label={t('bookmarks.search')} placeholder={t('bookmarks.searchPlaceholder')} value={query} onChange={setQuery}/><div className="toolbar__filters"><label>{t('bookmarks.status')} <select aria-label={t('bookmarks.statusFilter')} value={status} onChange={e => setStatus(e.target.value)}><option value="all">{t('bookmarks.allStatus')}</option>{STATUSES.map(id => <option value={id} key={id}>{label(id)}</option>)}</select></label><label>{t('common.tag')} <select aria-label={t('bookmarks.tagFilter')} value={tag} onChange={e=>setTag(e.target.value)}><option value="">{t('common.allTags')}</option>{[...new Set(items.flatMap(i=>i.tags||[]))].sort().map(name=><option key={name}>{name}</option>)}</select></label><label>{t('common.sort')} <select aria-label={t('bookmarks.sort')} value={sort} onChange={e=>setSort(e.target.value)}><option value="created">{t('bookmarks.sort.created')}</option><option value="updated">{t('bookmarks.sort.updated')}</option><option value="name">{t('bookmarks.sort.name')}</option></select></label></div></div>

    {error && !editing && <p role="alert" className="text-error">{t.backend(error)}</p>}
    <div className="library-list">{rows.map(({item,project,related})=><article className="bookmark-row" key={item?.id||project!.id}><div>
      <h3>{item?<a className="bookmark-link" href={item.url} target="_blank" rel="noreferrer" onClick={e=>{e.preventDefault();void api.openExternal(item.url).catch(e=>setError(String(e)));}}>{item.name}</a>:project?.name}</h3>
      {item&&<><p className="library-path">{item.url}</p><div className="bookmark-meta"><Badge tone={item.status==='in_use'?'green':item.status==='interested'?'blue':'neutral'}>{label(item.status)}</Badge>{item.tags?.map(name=><span className="bookmark-tag" key={name}>{name}</span>)}</div>{item.notes&&<p className="bookmark-notes">{item.notes}</p>}</>}
      {project&&<p className="bookmark-project"><span>{t('bookmarks.localProject',{name:project.name})}</span><code>{displayPath(project.path)}</code></p>}
      {related.map(other=><div className="bookmark-related" key={other.id}><a href={other.url} onClick={e=>{e.preventDefault();void api.openExternal(other.url).catch(e=>setError(String(e)));}}>{other.name}</a><span className="bookmark-related__meta">{[label(other.status),...(other.tags||[])].map(part=><span key={part}>{part}</span>)}</span>{other.notes&&<p>{other.notes}</p>}<button className="inline-link" onClick={()=>{setEditing({...other});setTagDraft((other.tags||[]).join(t.lang==='zh'?'，':', '));}}>{t('bookmarks.edit')}</button></div>)}
      </div><div className="bookmark-actions"><button className="button button--quiet" disabled={busy} onClick={()=>void mutate(()=>item?api.dispatch('bookmarks.save',{bookmark:{...item,archived:!item.archived}}):api.dispatch('projects.archive',{projectId:project!.id,archived:false}))}>{archived?t('common.unarchive'):t('common.archive')}</button>
      {item&&<><button className="button button--quiet" disabled={busy} onClick={()=>{setEditing({...item});setTagDraft((item.tags||[]).join(t.lang==='zh'?'，':', '));setError('');}} aria-label={t('bookmarks.editLabel',{name:item.name})}>{t('bookmarks.edit')}</button><button className="button button--quiet" disabled={busy} onClick={()=>void mutate(()=>api.dispatch('bookmarks.delete',{id:item.id}))} aria-label={t('bookmarks.deleteLabel',{name:item.name})}>{t('bookmarks.delete')}</button></>}
      {archived&&project&&<><button className="button button--quiet" disabled={busy} onClick={()=>onEditProject?.(project)}>{t('bookmarks.editProject')}</button><button className="button button--quiet" disabled={busy} onClick={()=>onDeleteProject?.(project)}>{t('bookmarks.deleteProject')}</button></>}
      </div></article>)}</div>
    {!rows.length&&<p className="empty-state">{archived?t('bookmarks.emptyArchived'):items.length?t('bookmarks.emptyFiltered'):t('bookmarks.empty')}</p>}
    {editing && <Modal title={editing.id ? t('bookmarks.editTitle') : t('bookmarks.addTitle')} onClose={()=>{if(!busy)setEditing(undefined);}} footer={<button className="button button--primary" disabled={busy || !editing.url.trim()} type="submit" form={formId}>{busy ? t('common.saving') : t('bookmarks.save')}</button>}><form id={formId} onSubmit={e => { e.preventDefault(); void mutate(() => api.dispatch('bookmarks.save', { bookmark: { ...editing, ...(tagDraft || editing.tags ? {tags:tagDraft.split(/[,，]/).map(part=>part.trim()).filter(Boolean)} : {}) } })); }}><label className="field"><span>{t('bookmarks.url')}</span><input type="url" required autoFocus value={editing.url} onChange={e => setEditing({ ...editing, url: e.target.value })}/></label><label className="field"><span>{t('bookmarks.nameOptional')}</span><input value={editing.name} onChange={e => setEditing({ ...editing, name: e.target.value })}/></label><label className="field"><span>{t('bookmarks.notes')}</span><textarea rows={4} value={editing.notes} onChange={e => setEditing({ ...editing, notes: e.target.value })}/></label><label className="field"><span>{t('common.tagsComma')}</span><input placeholder={t('bookmarks.tagPlaceholder')} value={tagDraft} onChange={e=>setTagDraft(e.target.value)}/></label><label className="field"><span>{t('bookmarks.statusField')}</span><select disabled={!!editing.projectId} value={editing.projectId?'in_use':editing.status} onChange={e => setEditing({ ...editing, status: e.target.value as RepositoryBookmark['status'] })}>{STATUSES.map(id => <option value={id} key={id}>{label(id)}</option>)}</select></label><label className="field"><span>{t('bookmarks.linkProject')}</span><select value={editing.projectId || ''} onChange={e => setEditing({ ...editing, projectId: e.target.value || null, status:e.target.value?'in_use':editing.status })}><option value="">{t('bookmarks.noLink')}</option>{editing.projectId && !projects.some(p => p.id === editing.projectId) && <option value={editing.projectId}>{t('bookmarks.deletedProject')}</option>}{projects.map(p => <option key={p.id} value={p.id}>{p.name}</option>)}</select></label>{error && <p role="alert" className="text-error">{t.backend(error)}</p>}</form></Modal>}
  </section>;
}
