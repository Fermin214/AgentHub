import {useEffect,useMemo,useState} from 'react';
import * as api from './api';
import {agentLabels} from './types';
import {useT} from './i18n';
export function useAgentNames(){
 const t=useT();
 const fallback=useMemo(()=>agentLabels(t),[t]);
 const [names,setNames]=useState<Record<string,string>>({});
 useEffect(()=>{let active=true;api.dispatch('targets.list').then(result=>{if(active && result?.targets)setNames(Object.fromEntries(result.targets.map(t=>[t.id,t.name])));}).catch(()=>{});return()=>{active=false;};},[]);
 return (id:string)=>names[id] || fallback[id] || id;
}
