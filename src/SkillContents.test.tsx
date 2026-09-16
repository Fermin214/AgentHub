import { act, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { expect, it, vi } from 'vitest';
import * as api from './api';
import { SkillContents } from './SkillContents';
import type { Skill } from './types';
it('ignores a delayed read after a different file is selected',async()=>{
  let resolve!:(value:{content:string})=>void;const slow=new Promise<{content:string}>(r=>{resolve=r;});
  const dispatch=vi.spyOn(api,'dispatch').mockImplementation(async (...[method,args]) =>{
    if(method==='skills.files')return {files:[{path:'SKILL.md',size:1},{path:'README.md',size:1}]} as never;
    if(method==='skills.read'&&args.path==='SKILL.md')return await slow as never;return {content:'Current README'} as never;
  });
  render(<SkillContents skill={{id:'s',name:'Writer'} as Skill}/>);
  await userEvent.click(await screen.findByRole('button',{name:'README.md'}));expect(await screen.findByText('Current README')).toBeVisible();
  await act(async()=>resolve({content:'Late Skill'}));expect(screen.queryByText('Late Skill')).not.toBeInTheDocument();
  expect(dispatch).toHaveBeenCalledWith('skills.read',{skillId:'s',path:'README.md'});dispatch.mockRestore();
});
it('reads the Skill library copy without offering an install location',async()=>{
  const dispatch=vi.spyOn(api,'dispatch').mockImplementation(async (...[method]) =>method==='skills.files'?{files:[{path:'SKILL.md',size:1}]} as never:{content:'Library body'} as never);
  render(<SkillContents skill={{id:'s',name:'Writer'} as Skill}/>);
  expect(await screen.findByText('Library body')).toBeVisible();
  expect(screen.queryByLabelText('阅读位置')).not.toBeInTheDocument();
  expect(screen.queryByRole('combobox')).not.toBeInTheDocument();
  for(const [,args] of dispatch.mock.calls)expect(args).not.toHaveProperty('deploymentId');
  expect(dispatch).toHaveBeenCalledWith('skills.files',{skillId:'s'});
  dispatch.mockRestore();
});
it('reports a library read failure without falling back to an installed copy',async()=>{
  const dispatch=vi.spyOn(api,'dispatch').mockImplementation(async (...[method]) =>{if(method==='skills.files')throw Error('Skill 库文件缺失');return {content:'Installed body'} as never;});
  render(<SkillContents skill={{id:'s',name:'Writer'} as Skill}/>);
  expect(await screen.findByRole('alert')).toHaveTextContent('Skill 库文件缺失');
  expect(screen.queryByText('Installed body')).not.toBeInTheDocument();
  expect(dispatch.mock.calls.flatMap(([m,args])=>m==='skills.read'?[args]:[])).toEqual([]);
  dispatch.mockRestore();
});
