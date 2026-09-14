import { useState } from 'react';
import * as api from './api';
import { displayPath } from './displayPath';
import { Button, Modal } from './ui';
import type { ChangeResult, SkillChangePlan } from './types';
import { useT } from './i18n';
import { useAgentNames } from './useAgentNames';
export function SkillChangeDialog({plan,onClose,onComplete,onReplaceModified}:{plan:SkillChangePlan;onClose:()=>void;onComplete:(result:ChangeResult)=>Promise<void>;onReplaceModified?:()=>Promise<void>}) {
  const t=useT();
  const [busy,setBusy]=useState(false);const [error,setError]=useState('');const [finished,setFinished]=useState(false);
  const agentName=useAgentNames();
  const title=t('change.'+plan.action);
  const cancel=async()=>{setBusy(true);try{if(!finished)await api.dispatch('skills.cancel',{planId:plan.id});onClose();}catch(e){setError(String(e));}finally{setBusy(false);}};
  const execute=async()=>{if(busy||finished)return;setBusy(true);setError('');try{const result=await api.dispatch(`skills.${plan.action}`,{planId:plan.id,confirmed:true});setFinished(true);await onComplete(result);if(result.status!=='succeeded')setError(result.summary);else onClose();}catch(e){setError(String(e));}finally{setBusy(false);}};
  const agents=[...new Set([...(plan.agents||[]),...plan.locations.flatMap(l=>l.agents)])];
  return <Modal title={title} wide onClose={busy?()=>{}:()=>void cancel()} footer={<><Button disabled={busy} onClick={()=>void cancel()}>{finished?t('common.close'):t('common.cancel')}</Button><Button variant={plan.action==='delete'?'danger':'primary'} loading={busy} disabled={!plan.canExecute||finished} onClick={()=>void execute()}>{t('change.confirm',{action:title})}</Button></>}><p>{t.backend(plan.summary)}</p>{agents.length>0&&<p>{t('change.affected',{agents:agents.map(agentName).sort(t.compare).join(t.lang==='zh'?'、':', ')})}{agents.length>1?t('change.sharedDirectory'):''}</p>}<ul className="skill-lifecycle__locations">{plan.locations.map(l=><li key={l.id}><strong>{t.backend(l.label)}</strong><span>{displayPath(l.path)}</span>{l.differences.length>0&&<small>{t('change.fileChanges',{n:l.differences.length})}</small>}</li>)}</ul>{plan.blockedReason&&<p role="alert" className="text-error">{t.backend(plan.blockedReason)}</p>}{onReplaceModified&&!plan.canExecute&&<Button disabled={busy} onClick={()=>{setBusy(true);void onReplaceModified().catch(e=>setError(String(e))).finally(()=>setBusy(false));}}>{t('change.allowOverwrite')}</Button>}{error&&<p role="alert" className="text-error">{t.backend(error)}</p>}</Modal>;
}
