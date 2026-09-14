import {useEffect,useState} from 'react';
import * as api from './api';
import {useT,type Translate} from './i18n';
import type { MaintenancePolicy, MaintenancePatch } from './contracts';
export type { MaintenancePolicy } from './contracts';
type Notify=(message:string,tone?:'success'|'error'|'info')=>void;
// Core joins its own failure list with a separator; rejoin it in the reader's language.
const joinFailures=(failures:Array<{message:string}>,t:Translate)=>failures.map(f=>t.backend(f.message)).join(t.lang==='zh'?'；':'; ');
export function DependencyStatus(){
  const t=useT();
  const [missing,setMissing]=useState(false);
  const [error,setError]=useState('');
  useEffect(()=>{let alive=true;api.dispatch('git.probe').then(r=>{if(alive)setMissing(r.available===false);}).catch(e=>{if(alive)setError(String(e));});return()=>{alive=false;};},[]);
  if(!missing&&!error)return null;
  return <div className="dependency-notice" role="status">{error?t.backend(error):<>{t('prefs.gitMissing')}<button className="inline-link" onClick={()=>void api.openExternal('https://git-scm.com/downloads/win').catch(e=>setError(String(e)))}>{t('prefs.gitDownload')}</button></>}</div>;
}
export function NetworkSettings({notify}:{notify:Notify}){
  const t=useT();
  const [proxy,setProxy]=useState('');const [saved,setSaved]=useState('');const [busy,setBusy]=useState(false);const [error,setError]=useState('');
  useEffect(()=>{let alive=true;api.dispatch('network.get').then(result=>{if(alive){setProxy(result.proxyUrl);setSaved(result.proxyUrl);}}).catch(e=>{if(alive)setError(String(e));});return()=>{alive=false;};},[]);
  const save=async()=>{setBusy(true);setError('');try{const result=await api.dispatch('network.save',{proxyUrl:proxy});setProxy(result.proxyUrl);setSaved(result.proxyUrl);notify(t('prefs.network.saved'));}catch(e){setError(String(e));}finally{setBusy(false);}};
  return <section className="settings-card"><div className="settings-card__heading"><div><h2>{t('prefs.network.title')}</h2><p>{t('prefs.network.body')}</p></div></div><label className="field"><span>{t('prefs.network.proxy')}</span><input aria-label={t('prefs.network.proxyLabel')} value={proxy} onChange={e=>setProxy(e.target.value)} placeholder={t('prefs.network.proxyPlaceholder')}/></label><p className="form-help">{t('prefs.network.help')}</p>{error && <p role="alert" className="text-error">{t.backend(error)}</p>}<button className="button button--primary" disabled={busy || proxy===saved} onClick={()=>void save()}>{t('prefs.network.save')}</button></section>;
}
export function MaintenanceSettings({notify}:{notify:Notify}){
  const t=useT();
  const [policy,setPolicy]=useState<MaintenancePolicy>({automaticChecks:false,intervalHours:24,retainUpdateBackup:true});const [interval,setInterval]=useState(24);const [busy,setBusy]=useState(false);const [error,setError]=useState('');
  useEffect(()=>{let alive=true;api.dispatch('maintenance.get').then(result=>{if(alive){setPolicy(result);setInterval(result.intervalHours);}}).catch(e=>{if(alive)setError(String(e));});return()=>{alive=false;};},[]);
  const save=async(patch:MaintenancePatch)=>{setBusy(true);setError('');try{const result=await api.dispatch('maintenance.save',patch);setPolicy(result);window.dispatchEvent(new Event('agenthub-maintenance-changed'));notify(t('prefs.updates.saved'));}catch(e){setError(String(e));}finally{setBusy(false);}};
  return <section className="settings-card"><div className="settings-card__heading"><div><h2>{t('prefs.updates.title')}</h2><p>{t('prefs.updates.body')}</p></div></div><label className="check-row"><input type="checkbox" disabled={busy} checked={policy.automaticChecks} onChange={e=>void save({automaticChecks:e.target.checked})}/>{t('prefs.updates.enable')}</label><label className="check-row"><input type="checkbox" disabled={busy} checked={policy.retainUpdateBackup!==false} onChange={e=>void save({retainUpdateBackup:e.target.checked})}/>{t('prefs.updates.retain')}</label><label className="field"><span>{t('prefs.updates.interval')}</span><input type="number" min={1} max={168} value={interval} onChange={e=>setInterval(Number(e.target.value))}/></label>{policy.lastAttemptAt&&<p className="form-help">{t('prefs.updates.lastCheck',{time:t.dateTime(policy.lastAttemptAt)})}</p>}{error&&<p role="alert" className="text-error">{t.backend(error)}</p>}<button className="button button--primary" disabled={busy||interval===policy.intervalHours||interval<1||interval>168||!Number.isInteger(interval)} onClick={()=>void save({intervalHours:interval})}>{t('prefs.updates.save')}</button></section>;
}
export function BackupRetentionSettings({notify}:{notify:Notify}){
  const t=useT();
  const [saved,setSaved]=useState<number|null>();
  const [value,setValue]=useState('');
  const [preview,setPreview]=useState<{maxBackups:number|null;pruneCount:number;protectedCount:number}>();
  const [busy,setBusy]=useState(false);
  const [error,setError]=useState('');
  const limit=value.trim()===''?null:Number(value);
  const valid=limit===null||(Number.isSafeInteger(limit)&&limit>0);
  useEffect(()=>{let live=true;api.dispatch('maintenance.get').then(r=>{if(live){setSaved(r.maxBackups??null);setValue(r.maxBackups==null?'':String(r.maxBackups));if(r.lastRetention?.status==='partial')setError(t('prefs.backups.lastPartial',{details:joinFailures(r.lastRetention.failures,t)}));}}).catch(e=>{if(live)setError(String(e));});return()=>{live=false;};},[]);
  useEffect(()=>{setPreview(undefined);if(saved===undefined||!valid)return;let live=true;api.dispatch('maintenance.preview',{maxBackups:limit}).then(r=>{if(live)setPreview(r);}).catch(e=>{if(live)setError(String(e));});return()=>{live=false;};},[limit,saved,valid]);
  const save=async()=>{setBusy(true);setError('');try{const r=await api.dispatch('maintenance.save',{maxBackups:limit});setSaved(r.maxBackups??null);setPreview(await api.dispatch('maintenance.preview',{maxBackups:r.maxBackups??null}));window.dispatchEvent(new Event('agenthub-backups-changed'));if(!r.retention)throw new Error('Backup retention result is missing');if(r.retention.failures.length)setError(t('prefs.backups.partial',{n:r.retention.failures.length,details:joinFailures(r.retention.failures,t)}));else notify(t('prefs.backups.savedToast',{n:r.retention.removedCount}));}catch(e){setError(String(e));}finally{setBusy(false);}};
  return <section className="settings-card"><h2>{t('prefs.backups.title')}</h2><p>{t('prefs.backups.body')}</p><label className="field"><span>{t('prefs.backups.field')}</span><input aria-label={t('prefs.backups.label')} type="number" min={1} step={1} value={value} placeholder={t('prefs.backups.unlimited')} onChange={e=>{setValue(e.target.value);setError('');}}/></label>{preview&&<p className="form-help">{t('prefs.backups.preview',{n:preview.pruneCount})}{preview.protectedCount>0?t('prefs.backups.protected',{n:preview.protectedCount}):''}</p>}{!valid&&<p className="text-error">{t('prefs.backups.invalid')}</p>}{error&&<p role="alert" className="text-error">{t.backend(error)}</p>}<button className="button button--primary" disabled={busy||!valid||!preview||(limit===saved&&!error&&preview.pruneCount===0)} onClick={()=>void save()}>{t('prefs.backups.save')}</button></section>;
}
