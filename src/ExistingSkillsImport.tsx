import { useEffect, useState } from 'react';
import * as api from './api';
import { displayPath } from './displayPath';
import { useAgentNames } from './useAgentNames';
import { useT } from './i18n';
import type { SkillDeployment } from './types';
import { Button } from './ui';
export function ExistingSkillsImport({deployments,refresh,onClose,onBusyChange}:{deployments:SkillDeployment[];refresh:()=>Promise<void>;onClose:()=>void;onBusyChange?:(busy:boolean)=>void}) {
  const t=useT();
  const [selected,setSelected]=useState<string[]>([]);const [ignored,setIgnored]=useState(false);const [busy,setBusy]=useState(false);const [error,setError]=useState('');const [warnings,setWarnings]=useState<string[]>([]);
  const agentName=useAgentNames();
  useEffect(()=>{onBusyChange?.(busy);return()=>onBusyChange?.(false);},[busy,onBusyChange]);
  const items=deployments.filter(d=>!d.skillId&&d.owner!=='host'&&(ignored||!d.ignored));
  const ids=selected.filter(id=>items.some(d=>d.id===id));
  const work=async(action:'scan'|'import'|'ignore'|'unignore')=>{setBusy(true);setError('');try{
    if(action==='scan'){const result=await api.scan();setWarnings(result.warnings);}
    else if(action==='import'){const result=await api.dispatch('skills.import',{deploymentIds:ids});const failed=result.results.filter(r=>r.status==='failed');if(failed.length)throw new Error(failed.map(r=>r.message||t('import.failed')).join('\n'));}
    else await api.dispatch('skills.ignore',{deploymentIds:ids,ignored:action==='ignore'});
    await refresh();setSelected([]);if(action==='import')onClose();
  }catch(e){setError(String(e));await refresh().catch(()=>{});}finally{setBusy(false);}};
  return <section className="existing-import">
    <div className="import-controls">
      <div className="import-controls__row"><Button loading={busy} onClick={()=>void work('scan')}>{t('import.scan')}</Button><label className="check-row"><input type="checkbox" disabled={busy} checked={ignored} onChange={e=>setIgnored(e.target.checked)}/>{t('import.showIgnored')}</label></div>
      <div className="import-controls__row"><label className="check-row"><input type="checkbox" aria-label={t('import.selectAll')} ref={el=>{if(el)el.indeterminate=ids.length>0&&ids.length<items.length;}} disabled={busy||!items.length} checked={items.length>0&&ids.length===items.length} onChange={e=>setSelected(e.target.checked?items.map(d=>d.id):[])}/>{t('import.selectAllShort')} <span className="selection-count">{ids.length} / {items.length}</span></label><div className="library-actions"><Button disabled={busy||!ids.length} onClick={()=>void work('ignore')}>{t('import.ignoreSelected')}</Button>{ignored&&<Button disabled={busy||!ids.length} onClick={()=>void work('unignore')}>{t('import.unignore')}</Button>}<Button variant="primary" disabled={busy||!ids.length} onClick={()=>void work('import')}>{t('import.addCount',{n:ids.length})}</Button></div></div>
    </div>
    {error&&<p role="alert" className="text-error">{t.backend(error)}</p>}
    {items.map(d=><label className="import-skills__item" key={d.id}><input type="checkbox" disabled={busy} checked={ids.includes(d.id)} onChange={e=>setSelected(v=>e.target.checked?[...v,d.id]:v.filter(id=>id!==d.id))}/><span><span className="candidate-heading"><strong>{d.name}</strong><span className="agent-label">{agentName(d.agent)}</span>{d.ignored&&<span className="badge badge--neutral">{t('import.ignored')}</span>}</span><small className="library-path">{displayPath(d.path)}</small><small>{d.description}</small></span></label>)}
    {!items.length&&<p className="empty-state">{t('import.empty')}</p>}{warnings.length>0&&<details><summary>{t('import.scanDetails')}</summary>{warnings.map((w,i)=><p key={i}>{t.backend(w)}</p>)}</details>}
  </section>;
}
