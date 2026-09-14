import { expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { SkillAgentIcons, agentLocations } from './SkillAgentIcons';
import type { Skill, SkillDeployment, LocalProject } from './types';
const skill={id:'s',name:'Writer'} as Skill;
const deployment={id:'d',skillId:'s',agent:'codex',scope:'global',path:'C:/shared/skills/writer',present:true,owner:'external'} as SkillDeployment;
const targets=[{id:'codex',name:'Codex',globalPath:'C:/shared/skills',projectPath:'.agents/skills',enabled:true},{id:'claude',name:'Claude Code',globalPath:'C:/shared/skills',projectPath:'.claude/skills',enabled:true}];
it('marks shared physical directories together and requests a reviewed remove',async()=>{
  const onChange=vi.fn();render(<SkillAgentIcons skill={skill} deployments={[deployment]} targets={targets} busy={false} onChange={onChange}/>);
  const buttons=screen.getAllByRole('button');expect(buttons[0]).toHaveAccessibleName('从 Claude Code 移除 Writer');
  expect(buttons.every(b=>b.getAttribute('aria-pressed')==='true')).toBe(true);
  await userEvent.click(buttons[0]);expect(onChange).toHaveBeenCalledWith('remove',{deploymentId:'d'});
});
it('keeps global and other project deployments out of the selected project scope',()=>{
  const project={id:'p',path:'C:/project'} as LocalProject;
  const unrelated={...deployment,id:'other',scope:'project',projectId:'other',path:'C:/other/.agents/skills/writer'} as SkillDeployment;
  expect(agentLocations('codex',[deployment,unrelated],targets,project)).toEqual([]);
  expect(agentLocations('codex',[unrelated],targets)).toEqual([]);
});
