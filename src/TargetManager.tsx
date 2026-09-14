import {useEffect,useState} from 'react';
import {open} from '@tauri-apps/plugin-dialog';
import * as api from './api';
import {displayPath} from './displayPath';
import {AgentIcon} from './AgentIcon';
import {useT} from './i18n';
import type { AgentTarget } from './contracts';
export type { AgentTarget } from './contracts';
type Notify=(message:string,tone?:'success'|'error'|'info')=>void;
export function TargetManager({notify,onChanged}:{notify:Notify;onChanged?:()=>void}){
  const t=useT();
  const [targets,setTargets]=useState<AgentTarget[]>([]);
  const [editing,setEditing]=useState<AgentTarget>();
  const [busy,setBusy]=useState(false);const [error,setError]=useState('');
  const load=async()=>{const r=await api.dispatch('targets.list');setTargets([...(r.targets||[])].sort((a,b)=>t.compare(a.name,b.name)));};
  useEffect(()=>{void load().catch(e=>setError(String(e)));},[]);
  const save=async(target:AgentTarget)=>{setBusy(true);setError('');try{await api.dispatch('targets.save',{target});await load();setEditing(undefined);onChanged?.();notify(t('targets.savedToast'));}catch(e){setError(String(e));}finally{setBusy(false);}};
  const pick=async()=>{if(!api.isTauriRuntime())return;try{const p=await open({directory:true,multiple:false});if(typeof p==='string')setEditing(v=>v?{...v,globalPath:p}:v);}catch(e){setError(String(e));}};
  return <section className="settings-card"><div className="settings-card__heading"><div><h2>{t('targets.title')}</h2><p>{t('targets.body')}</p></div><button className="button button--secondary" disabled={busy} onClick={()=>void load().catch(e=>setError(String(e)))}>{t('targets.redetect')}</button></div>
    {error&&!editing&&<p role="alert" className="text-error">{t.backend(error)}</p>}
    <div className="agent-settings-list">{targets.map(target=><div className="scan-root-row" key={target.id}><AgentIcon agent={target.id} name={target.name}/><div><strong>{target.name}</strong><small className="agent-discovery-state">{t(target.available?'targets.detected':'targets.notDetected')}</small><p className="library-path" title={target.globalPath}>{t('targets.allProjects',{path:displayPath(target.globalPath)||t('targets.noPath')})}</p><small>{t('targets.perProject',{path:target.projectPath||t('targets.noProjectPath')})}</small></div><label className="check-row"><input aria-label={t('targets.manageLabel',{name:target.name})} type="checkbox" checked={target.enabled} disabled={busy} onChange={e=>void save({...target,enabled:e.target.checked})}/>{t('targets.manage')}</label><button className="button button--quiet" disabled={busy} onClick={()=>{setEditing(target);setError('');}}>{t('targets.editLocation')}</button></div>)}</div>
    {editing&&<div className="modal-backdrop"><section className="modal" role="dialog" aria-modal="true" aria-labelledby="target-title"><header className="modal__header"><h2 id="target-title">{t('targets.editTitle',{name:editing.name})}</h2><button className="icon-button" aria-label={t('targets.closeDialog')} disabled={busy} onClick={()=>setEditing(undefined)}>×</button></header><div className="modal__body">{error&&<p role="alert" className="text-error">{t.backend(error)}</p>}<label className="field"><span>{t('targets.globalField')}</span><div className="toolkit-path-picker"><input value={editing.globalPath} onChange={e=>setEditing({...editing,globalPath:e.target.value})}/><button className="button button--secondary" onClick={()=>void pick()}>{t('common.chooseFolder')}</button></div></label><label className="field"><span>{t('targets.projectField')}</span><input value={editing.projectPath} onChange={e=>setEditing({...editing,projectPath:e.target.value})}/></label><p className="form-help">{t('targets.projectHelp')}</p></div><footer className="modal__footer"><button className="button button--secondary" disabled={busy} onClick={()=>setEditing(undefined)}>{t('common.cancel')}</button><button className="button button--primary" disabled={busy||!editing.globalPath.trim()} onClick={()=>void save(editing)}>{t('targets.save')}</button></footer></section></div>}
  </section>;
}
