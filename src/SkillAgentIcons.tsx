import type { SkillReviewCall } from './contracts';
import { useState } from 'react';
import { Check } from 'lucide-react';
import { AgentIcon } from './AgentIcon';
import { displayPath } from './displayPath';
import type { AgentTarget } from './TargetManager';
import type { Skill, SkillChangeAction, SkillDeployment, LocalProject } from './types';
import { AGENT_LABELS } from './types';
import { useT } from './i18n';
import { Button, Modal } from './ui';
const locationKey = (path:string) => path.replace(/^\\\\\?\\/,'').replace(/\\/g,'/').replace(/\/+$/,'').toLowerCase();
export const skillPresent = (d:SkillDeployment) => d.present ?? d.status !== 'missing';
export function agentLocations(agent:string, deployments:SkillDeployment[], targets:AgentTarget[], project?:LocalProject) {
  const target=targets.find(t=>t.id===agent);
  const root=project ? target?.projectPath ? locationKey(project.path+'/'+target.projectPath) : '' : locationKey(target?.globalPath||'');
  return [...new Map(deployments.filter(d=>project ? d.projectId===project.id || (d.scope==='project'&&!!root&&locationKey(d.path).slice(0,locationKey(d.path).lastIndexOf('/'))===root) : d.scope!=='project').filter(d=>d.agent===agent || (!!root&&locationKey(d.path).slice(0,locationKey(d.path).lastIndexOf('/'))===root)).map(d=>[d.pathKey||locationKey(d.path),d])).values()];
}
export function SkillAgentIcons({skill,deployments,targets,project,busy,onChange}:{skill:Skill;deployments:SkillDeployment[];targets:AgentTarget[];project?:LocalProject;busy:boolean;onChange:(...call:SkillReviewCall)=>void}) {
  const t=useT();
  const [choosing,setChoosing]=useState<AgentTarget>();
  const inScope=deployments.filter(d=>project?d.projectId===project.id:d.scope!=='project').filter(skillPresent);
  const agents=targets.filter(target=>target.enabled||agentLocations(target.id,deployments,targets,project).some(skillPresent));
  for(const d of inScope){if(agents.some(target=>agentLocations(target.id,[d],targets,project).length))continue;if(!agents.some(target=>target.id===d.agent))agents.push({id:d.agent,name:AGENT_LABELS[d.agent]||d.agent,globalPath:'',projectPath:'',enabled:true});}
  agents.sort((a,b)=>t.compare(a.name,b.name));
  return <div className="skill-sync"><div className="skill-sync__agents" aria-label={t('skills.agents.label',{name:skill.name})}>{agents.map(target=>{
    const locations=agentLocations(target.id,deployments,targets,project).filter(skillPresent);
    const active=locations.length>0;const immutable=locations.length===1&&locations[0].owner==='host';
    const disabled=busy||immutable||(!active&&!(project?target.projectPath:target.globalPath));
    return <button key={target.id} className={'skill-sync__agent'+(active?' is-synced':'')} aria-label={t(active?'skills.agents.remove':'skills.agents.install',{agent:target.name,name:skill.name})} aria-pressed={active} title={immutable?t('skills.agents.hostManaged'):target.name+' · '+(active?t('skills.agents.installed'):t('skills.agents.notInstalled'))} disabled={disabled} onClick={()=>locations.length>1?setChoosing(target):active?onChange('remove',{deploymentId:locations[0].id}):onChange('install',{skillId:skill.id,targetId:target.id,...(project?{projectId:project.id}:{})})}><AgentIcon agent={target.id} name={target.name} size={16}/>{active&&<Check className="skill-sync__check" size={10}/>}</button>;
  })}</div>{choosing&&<Modal title={skill.name+' · '+choosing.name} onClose={()=>setChoosing(undefined)}><p>{t('skills.agents.chooseRemoval')}</p>{agentLocations(choosing.id,deployments,targets,project).filter(skillPresent).map(d=><div className="sync-location" key={d.id}><div><strong>{d.profile||choosing.name}</strong><span className="library-path">{displayPath(d.path)}</span></div><Button disabled={busy||d.owner==='host'} onClick={()=>{setChoosing(undefined);onChange('remove',{deploymentId:d.id});}}>{t('skills.agents.removeThis')}</Button></div>)}</Modal>}</div>;
}
