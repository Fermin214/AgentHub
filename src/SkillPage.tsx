import type { InstallRequest, SkillReviewCall } from './contracts';
import { checkError } from './checkError';
import { useEffect, useState } from 'react';
import { Plus, Star, RefreshCw } from 'lucide-react';
import * as api from './api';
import { AddSkillDialog } from './AddSkillDialog';
import { RepositoryManager } from './RepositoryManager';
import { SkillAgentIcons, agentLocations, skillPresent } from './SkillAgentIcons';
import { SkillChangeDialog } from './SkillChangeDialog';
import { SkillContents } from './SkillContents';
import { sourceWebUrl } from './sourceWebUrl';
import { SkillUpdateDialog } from './SkillUpdateDialog';
import type { SkillUpdateController } from './useSkillUpdates';
import { Button, Modal, PageHeader, type Notify, SearchField } from './ui';
import { useT } from './i18n';
import type { AgentTarget } from './TargetManager';
import type { ChangeResult, Skill, SkillChangeAction, SkillChangePlan, Snapshot } from './types';
type Props={snapshot:Snapshot;refresh:()=>Promise<void>;notify:Notify;controller:SkillUpdateController;projectId?:string;onProject:(id?:string)=>void;onSkills?:(saved:Skill[],fields:Array<'favorite'|'tags'>)=>void};
export function SkillPage({snapshot,refresh,notify,controller,projectId,onProject,onSkills}:Props) {
  const t=useT();
  const [installing,setInstalling]=useState(false);const [installQuery,setInstallQuery]=useState('');
  const [query,setQuery]=useState('');const [tag,setTag]=useState('');const [favorite,setFavorite]=useState(false);const [agent,setAgent]=useState('');
  const [targets,setTargets]=useState<AgentTarget[]>([]);const [adding,setAdding]=useState(false);const [binding,setBinding]=useState<Skill>();const [repositories,setRepositories]=useState(false);const [reading,setReading]=useState<Skill>();const [metadata,setMetadata]=useState<Skill>();const [tags,setTags]=useState('');const [deleting,setDeleting]=useState<Skill>();const [removeDeployments,setRemoveDeployments]=useState(false);
  const [updating,setUpdating]=useState<Skill>();const [plan,setPlan]=useState<SkillChangePlan>();const [error,setError]=useState('');
  // Waiting state is owned per operation. `busyCount` counts only the operations on
  // this page (review preview, delete, tags, unbind); a favorite save never touches
  // it, so favorite work can neither hold nor release another operation's protection.
  const [busyCount,setBusyCount]=useState(0);
  const busy=busyCount>0;
  const beginBusy=()=>setBusyCount(count=>count+1);
  const endBusy=()=>setBusyCount(count=>count-1);
  // Three separate favorite concerns, so neither one can lock the row:
  // - `favoriteRequests` is the in-flight request lock (dedupe only, per Skill id);
  // - `favoriteValues` holds the value the store committed, shown until a snapshot
  //   confirms it, so a slow or failed reload cannot revive the previous value;
  // - an effect over the *current* snapshot retires that value once the record agrees.
  const [favoriteRequests,setFavoriteRequests]=useState<string[]>([]);
  const [favoriteValues,setFavoriteValues]=useState<Record<string,{value:boolean;reported:boolean}>>({});
  const favoriteOf=(skill:Skill)=>favoriteValues[skill.id]?.value??skill.favorite;
  const favoritePending=(skill:Skill)=>favoriteRequests.includes(skill.id);
  useEffect(()=>{
    setFavoriteValues(values=>{
      // Retire the committed value once the record supports it. Before a save has been
      // reported, only an exact match counts, so the value that prompted the save still
      // wins; after it is reported, any snapshot newer than that save is authoritative.
      const settled=Object.entries(values).filter(([id,entry])=>{
        const record=snapshot.skills.find(item=>item.id===id);
        return !!record&&(record.favorite===entry.value||entry.reported);
      }).map(([id])=>id);
      if(!settled.length)return values;
      const next={...values};for(const id of settled)delete next[id];return next;
    });
  },[snapshot]);
  const readingSource = reading ? sourceWebUrl(reading.source) : undefined;
  const {checks,checkingIds,checkProgress}=controller;
  const [unbinding,setUnbinding]=useState<Skill>();
  const [deleteDetails,setDeleteDetails]=useState<SkillChangePlan>();
  const [installArgs,setInstallArgs]=useState<InstallRequest>();
  const project=snapshot.projects.find(p=>p.id===projectId&&!p.archived);
  useEffect(()=>{let live=true;api.dispatch('targets.list').then(r=>{if(live)setTargets(r.targets.sort((a,b)=>t.compare(a.name,b.name)));}).catch(e=>{if(live)setError(String(e));});return()=>{live=false;};},[snapshot.settings,t]);
  const deployments=(skill:Skill)=>snapshot.deployments.filter(d=>d.skillId===skill.id);
  const review=async(...[action,args]:SkillReviewCall)=>{beginBusy();setError('');try{setPlan(await (action==='install'?api.dispatch('skills.install.preview',args):api.dispatch('skills.remove.preview',args)));setInstallArgs(action==='install'?args:undefined);setDeleting(undefined);setUpdating(undefined);}catch(e){setError(String(e));}finally{endBusy();}};
  const check=(skill:Skill)=>{setError('');return controller.check(skill);};
  const checkAll=()=>{setError('');return controller.checkAll();};
  const deleteSkill=async()=>{if(!deleting)return;beginBusy();setError('');let pending:SkillChangePlan|undefined;try{pending=await api.dispatch('skills.delete.preview',{skillId:deleting.id,removeDeployments});if(!pending.canExecute)throw Error(pending.blockedReason||pending.summary);const result=await api.dispatch('skills.delete',{planId:pending.id,confirmed:true});if(result.status!=='succeeded')throw Error(result.summary);await refresh();setDeleting(undefined);notify(result.summary,'success');}catch(e){setError(String(e));}finally{if(pending)await api.dispatch('skills.cancel',{planId:pending.id}).catch(()=>{});endBusy();}};
  const showDeleteDetails=async()=>{if(!deleting)return;beginBusy();setError('');try{const p=await api.dispatch('skills.delete.preview',{skillId:deleting.id,removeDeployments});setDeleteDetails(p);await api.dispatch('skills.cancel',{planId:p.id});}catch(e){setError(String(e));}finally{endBusy();}};
  const saveMetadata=async(skill:Skill,patch:{favorite?:boolean;tags?:string[]},pending?:boolean)=>{
    // Only the in-flight request is a lock. A committed value is not, so a finished
    // save (successful or not) always leaves the row able to act again.
    if(pending&&favoritePending(skill))return;
    if(pending)setFavoriteRequests(ids=>[...ids,skill.id]);else beginBusy();
    setError('');
    let saved=false;
    try{
      const result=await api.dispatch('skills.metadata.save',{ids:[skill.id],...patch});
      saved=true;
      // Pass only the records this save actually committed. Never rebuild a "full"
      // list from this request's stale snapshot: when requests finish out of order,
      // that would hand the shell old copies of rows another save already updated.
      const records=(result?.skills??[]).filter(item=>item.id===skill.id);
      if(pending)setFavoriteValues(values=>({...values,[skill.id]:{value:!!(records.find(item=>item.id===skill.id)??{favorite:patch.favorite}).favorite,reported:!!onSkills}}));
      if(!pending)setMetadata(undefined);
      // A response contains the entire record at commit time. Only the fields this
      // request changed may replace current values: another save of the same Skill
      // may already have delivered newer tags or favorite state.
      const fields:Array<'favorite'|'tags'>=[];
      if(patch.favorite!==undefined)fields.push('favorite');
      if(patch.tags!==undefined)fields.push('tags');
      onSkills?.(records,fields);
    }catch(e){
      setError(String(e));
    }finally{if(pending)setFavoriteRequests(ids=>ids.filter(id=>id!==skill.id));else endBusy();}
    // The committed value stays authoritative until a snapshot supports it, so a reload
    // that is slow, stale or failed cannot revive the previous value. Retiring it is
    // the effect's job, because this request's captured snapshot is already stale.
    try{await refresh();}
    catch(e){
      // Only a committed save may report "saved but not reloaded"; a failed save
      // keeps its own accurate error.
      if(pending&&saved)notify(t('skills.favoriteSavedRefreshFailed'),'error');
      else if(!pending)setError(String(e));
    }
  };
  const toggleFavorite=(skill:Skill)=>{
    if(favoritePending(skill))return;
    void saveMetadata(skill,{favorite:!favoriteOf(skill)},true);
  };
  const visible=snapshot.skills.filter(s=>(!query||[s.name,s.description,...s.tags].join(' ').toLowerCase().includes(query.toLowerCase()))&&(!tag||s.tags.includes(tag))&&(!favorite||favoriteOf(s))&&(!project||deployments(s).some(d=>skillPresent(d)&&(d.projectId===project.id||(d.scope==='project'&&targets.some(target=>agentLocations(target.id,[d],targets,project).length>0)))))&&(!agent||agentLocations(agent,deployments(s),targets,project).some(skillPresent)));
  return <><PageHeader title={t('skills.title')} description={t('skills.description')} action={<>{project&&<Button onClick={()=>{setInstalling(true);setInstallQuery('');}}>{t('skills.installFromLibrary')}</Button>}<Button disabled={busy||!!checkProgress||!snapshot.skills.some(s=>s.source.kind!=='unknown'&&s.source.locator)} icon={<RefreshCw size={15}/>} onClick={()=>void checkAll()}>{checkProgress?t('skills.checking',{progress:checkProgress}):t('skills.checkAll')}</Button><Button disabled={busy} onClick={()=>setRepositories(true)}>{t('skills.repositories')}</Button><Button variant="primary" icon={<Plus size={15}/>} onClick={()=>setAdding(true)}>{t('skills.add')}</Button></>}/>
    <div className="toolbar"><SearchField label={t('skills.search')} placeholder={t('skills.searchPlaceholder')} value={query} onChange={setQuery}/><label>{t('skills.installScope')} <select aria-label={t('skills.installScopeLabel')} value={project?.id||''} onChange={e=>onProject(e.target.value||undefined)}><option value="">{t('skills.allProjects')}</option>{snapshot.projects.filter(p=>!p.archived).map(p=><option key={p.id} value={p.id}>{p.name}</option>)}</select></label><label>Agent <select value={agent} onChange={e=>setAgent(e.target.value)}><option value="">{t('common.all')}</option>{targets.map(target=><option key={target.id} value={target.id}>{target.name}</option>)}</select></label><label>{t('common.tag')} <select value={tag} onChange={e=>setTag(e.target.value)}><option value="">{t('common.all')}</option>{[...new Set(snapshot.skills.flatMap(s=>s.tags))].map(name=><option key={name}>{name}</option>)}</select></label><label><input type="checkbox" checked={favorite} onChange={e=>setFavorite(e.target.checked)}/>{t('common.favoritesOnly')}</label></div>
    {(error||controller.error)&&<p role="alert" className="text-error">{t.backend(error||controller.error)}</p>}<div className="library-list">{visible.map(skill=>{const update=checks[skill.id];const hasChanges=!!update?.locations?.some(l=>l.differences.length>0);const canCheck=skill.source.kind!=='unknown'&&!!skill.source.locator;return <article className="library-row" key={skill.id}><div className="library-row__main"><h3><button className="inline-link" onClick={()=>setReading(skill)}>{skill.name}</button></h3><p>{skill.description}</p><div className="tag-list">{skill.tags.map(name=><span key={name}>#{name}</span>)}</div><SkillAgentIcons skill={skill} deployments={deployments(skill)} targets={targets} project={project} busy={busy} onChange={(...call)=>void review(...call)}/><p className="form-help skill-check-summary"><span>{(update?.status==='failed'?checkError(update.message,t):update&&t.backend(update.message))||(canCheck?t('skills.notChecked'):t('skills.noSource'))}</span>{update?.checkedAt&&<span className="check-time">{t('skills.lastChecked',{time:t.dateTime(update.checkedAt)})}{update.needsRefresh?t('skills.needsRecheck'):''}</span>}</p>{update?.status==='failed'&&<details className="check-diagnostic"><summary>{t('common.errorDetails')}</summary><pre>{update.message}</pre></details>}</div><div className="library-actions"><button className="icon-button" data-favorite-pending={favoritePending(skill)?'true':undefined} aria-label={t(favoritePending(skill)?'skills.favoritePending':favoriteOf(skill)?'skills.unfavoriteLabel':'skills.favoriteLabel',{name:skill.name})} aria-busy={favoritePending(skill)||undefined} disabled={favoritePending(skill)} onClick={()=>toggleFavorite(skill)}><Star size={16} fill={favoriteOf(skill)?'currentColor':'none'}/></button><Button disabled={busy} onClick={()=>{setMetadata(skill);setTags(skill.tags.join(t.lang==='zh'?'，':', '));}}>{t('skills.tags')}</Button><Button disabled={busy} onClick={()=>canCheck?setUnbinding(skill):setBinding(skill)}>{canCheck?t('skills.removeSource'):t('skills.setSource')}</Button>{canCheck&&<Button variant={hasChanges?'primary':'secondary'} disabled={busy||checkingIds.includes(skill.id)} onClick={()=>hasChanges?setUpdating(skill):void check(skill)}>{checkingIds.includes(skill.id)?t('skills.checkingShort'):hasChanges?t('skills.update'):t('skills.checkUpdate')}</Button>}<Button disabled={busy} onClick={()=>{setDeleting(skill);setRemoveDeployments(false);setDeleteDetails(undefined);setError('');}}>{t('skills.deleteFromLibrary')}</Button></div></article>;})}</div>{!visible.length&&<div className="empty-state"><h3>{snapshot.skills.length?t('skills.empty.noMatch'):t('skills.empty.none')}</h3><p>{t('skills.empty.body')}</p></div>}
    {(adding||binding)&&<AddSkillDialog deployments={snapshot.deployments} refresh={refresh} bindSkill={binding} onClose={()=>{setAdding(false);setBinding(undefined);}}/>}{repositories&&<RepositoryManager skills={snapshot.skills} deployments={snapshot.deployments} refresh={refresh} onDelete={skill=>{setDeleting(skill);setRemoveDeployments(false);}} onClose={()=>setRepositories(false)}/>}
    {installing&&project&&<Modal title={t('skills.installToProject',{name:project.name})} wide onClose={()=>setInstalling(false)}><SearchField label={t('skills.searchLibrary')} placeholder={t('skills.searchPlaceholder')} value={installQuery} onChange={setInstallQuery}/><div className="library-list">{snapshot.skills.filter(s=>[s.name,s.description,...s.tags].join(' ').toLowerCase().includes(installQuery.toLowerCase())).map(skill=><article className="library-row" key={skill.id}><div className="library-row__main"><h3>{skill.name}</h3><p>{skill.description}</p><SkillAgentIcons skill={skill} deployments={deployments(skill)} targets={targets} project={project} busy={busy} onChange={(...call)=>{setInstalling(false);void review(...call);}}/></div></article>)}</div></Modal>}
    {unbinding&&<Modal title={t('skills.unbind.title',{name:unbinding.name})} onClose={busy?()=>{}:()=>setUnbinding(undefined)} footer={<Button variant="danger" loading={busy} onClick={()=>{beginBusy();setError('');void api.dispatch('skills.unbindSource',{skillId:unbinding.id}).then(async()=>{await refresh();setUnbinding(undefined);}).catch(e=>setError(String(e))).finally(()=>endBusy());}}>{t('skills.unbind.confirm')}</Button>}><p>{t('skills.unbind.body')}</p><p className="library-path">{unbinding.source.locator}{unbinding.source.subpath?' / '+unbinding.source.subpath:''}</p>{error&&<p role="alert" className="text-error">{t.backend(error)}</p>}</Modal>}
    {reading&&<Modal title={reading.name} titleAction={readingSource ? <a href={readingSource} onClick={event=>{event.preventDefault();void api.openExternal(readingSource).catch(error=>notify(String(error),'error'));}}>{t('contents.openSource')} ↗</a> : undefined} wide onClose={()=>setReading(undefined)}><SkillContents skill={reading}/></Modal>}
    {metadata&&<Modal title={t('skills.tagsDialog',{name:metadata.name})} onClose={()=>setMetadata(undefined)} footer={<Button variant="primary" loading={busy} onClick={()=>void saveMetadata(metadata,{tags:tags.split(/[,，]/).map(item=>item.trim()).filter(Boolean)})}>{t('skills.saveTags')}</Button>}><label className="field"><span>{t('common.tagsComma')}</span><input value={tags} onChange={e=>setTags(e.target.value)}/></label></Modal>}
    {deleting&&<Modal title={t('skills.delete.title',{name:deleting.name})} onClose={busy?()=>{}:()=>setDeleting(undefined)} footer={<><Button disabled={busy} onClick={()=>void showDeleteDetails()}>{t('skills.delete.showPaths')}</Button><Button variant="danger" loading={busy} onClick={()=>void deleteSkill()}>{t('skills.delete.confirm')}</Button></>}><p>{t('skills.delete.body')}</p><label><input type="checkbox" checked={removeDeployments} disabled={busy} onChange={e=>{setRemoveDeployments(e.target.checked);setDeleteDetails(undefined);}}/>{t('skills.delete.alsoRemove')}</label>{removeDeployments&&<p className="delete-scope">{t('skills.delete.willRemove',{agents:[...new Set(deployments(deleting).map(d=>targets.find(target=>target.id===d.agent)?.name||d.agent))].join(t.lang==='zh'?'、':', ')||t('skills.delete.noInstalls')})}</p>}{deleteDetails&&<ul className="skill-lifecycle__locations">{deleteDetails.locations.map(l=><li key={l.id}><strong>{l.label}</strong><span>{l.path}</span></li>)}</ul>}{error&&<p role="alert" className="text-error">{t.backend(error)}</p>}</Modal>}
    {updating&&<SkillUpdateDialog skill={updating} controller={controller} refresh={refresh} notify={notify} onClose={()=>setUpdating(undefined)}/>}
    {plan&&<SkillChangeDialog key={plan.id} plan={plan} onClose={()=>setPlan(undefined)} onReplaceModified={installArgs?async()=>{await api.dispatch('skills.cancel',{planId:plan.id});setPlan(await api.dispatch('skills.install.preview',{...installArgs,replaceModified:true}));}:undefined} onComplete={async result=>{await refresh();notify(result.summary,result.status==='succeeded'?'success':'error');}}/>}
  </>;
}
