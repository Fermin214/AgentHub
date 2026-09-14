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
  render(<SkillContents skill={{id:'s',name:'Writer'} as Skill} deployments={[]}/>);
  await userEvent.click(await screen.findByRole('button',{name:'README.md'}));expect(await screen.findByText('Current README')).toBeVisible();
  await act(async()=>resolve({content:'Late Skill'}));expect(screen.queryByText('Late Skill')).not.toBeInTheDocument();
  expect(dispatch).toHaveBeenCalledWith('skills.read',{skillId:'s',deploymentId:undefined,path:'README.md'});dispatch.mockRestore();
});
