import {useEffect,useRef} from 'react';
import * as api from './api';
import type {MaintenancePolicy} from './PreferencesPanel';
export function isCheckDue(policy:MaintenancePolicy,now=Date.now()){
  if(!policy.automaticChecks || !Number.isFinite(policy.intervalHours) || policy.intervalHours<1)return false;
  const last=policy.lastAttemptAt?Date.parse(policy.lastAttemptAt):0;
  return !last || now-last>=policy.intervalHours*3600000;
}
export function useAutomaticUpdates(scope:string|undefined,busy:boolean,check:()=>Promise<void>){
  const latest=useRef({busy,check});latest.current={busy,check};
  useEffect(()=>{
    if(!scope)return;
    let active=true;let ticking=false;
    const tick=async()=>{
      if(!active || ticking || latest.current.busy)return;
      ticking=true;
      try{
        const policy=await api.dispatch('maintenance.get');
        if(active && !latest.current.busy && isCheckDue(policy))await latest.current.check();
      }catch{/* Manual checks and Settings expose service errors; background work stays quiet. */}
      finally{ticking=false;}
    };
    const start=window.setTimeout(()=>void tick(),1500);
    const timer=window.setInterval(()=>void tick(),60000);
    const changed=()=>void tick();
    window.addEventListener('agenthub-maintenance-changed',changed);
    return()=>{active=false;window.clearTimeout(start);window.clearInterval(timer);window.removeEventListener('agenthub-maintenance-changed',changed);};
  },[scope]);
}