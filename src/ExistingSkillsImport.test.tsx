import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { expect, it, vi } from 'vitest';
import * as api from './api';
import { ExistingSkillsImport } from './ExistingSkillsImport';
import type { SkillDeployment } from './types';

it('selects only visible importable Skills and passes that selection to the action',async()=>{
  const dispatch=vi.spyOn(api,'dispatch').mockImplementation(async (...[method]) =>method==='targets.list'?{targets:[{id:'codex',name:'Codex'}]} as never:{} as never);
  const base={agent:'codex',name:'One',description:'',path:'C:/skills/one',owner:'external',ignored:false} as SkillDeployment;
  try{
    render(<ExistingSkillsImport deployments={[{...base,id:'one'},{...base,id:'two',name:'Two'},{...base,id:'ignored',ignored:true},{...base,id:'host',owner:'host'},{...base,id:'saved',skillId:'s'}]} refresh={vi.fn().mockResolvedValue(undefined)} onClose={vi.fn()}/>);
    await userEvent.click(screen.getByLabelText('全选本机 Skill'));
    expect(screen.getByRole('button',{name:'添加到 Skill 库 (2)'})).toBeEnabled();
    await userEvent.click(screen.getByRole('button',{name:'忽略所选'}));
    expect(dispatch).toHaveBeenCalledWith('skills.ignore',{deploymentIds:['one','two'],ignored:true});
  }finally{dispatch.mockRestore();}
});
