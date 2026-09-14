import { beforeEach,expect,it,vi } from 'vitest';
import { act,render,screen,waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { AddSkillDialog } from './AddSkillDialog';
import * as api from './api';
import type { Skill } from './types';
const skill:Skill={id:'canvas',name:'json-canvas',description:'',source:{kind:'unknown',locator:''},path:'C:/library/canvas',tags:[],favorite:false,createdAt:'',updatedAt:''};
beforeEach(()=>vi.restoreAllMocks());
it('shows only the current Skill and waits for Save before binding its source',async()=>{
  const dispatch=vi.spyOn(api,'dispatch').mockImplementation(async (...[method]) =>{
    if(method==='repositories.list')return {repositories:[]} as never;
    if(method==='sources.inspect')return {inspectionId:'inspection',source:{kind:'git',locator:'https://github.com/kepano/obsidian-skills'},candidates:[{name:'json-canvas',subpath:'skills/json-canvas'},{name:'obsidian-bases',subpath:'skills/obsidian-bases'}]} as never;
    return {} as never;
  });
  const onClose=vi.fn(),refresh=vi.fn().mockResolvedValue(undefined);
  render(<AddSkillDialog deployments={[]} bindSkill={skill} refresh={refresh} onClose={onClose}/>);
  await userEvent.type(screen.getByLabelText('Skill 来源'),'kepano/obsidian-skills');
  await userEvent.click(screen.getByRole('button',{name:'查找来源'}));
  expect(await screen.findAllByRole('radio')).toHaveLength(1);
  expect(screen.queryByText('obsidian-bases')).not.toBeInTheDocument();
  expect(dispatch.mock.calls.some(([m])=>m==='skills.bindSource')).toBe(false);
  expect(onClose).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole('button',{name:'保存来源'}));
  await waitFor(()=>expect(onClose).toHaveBeenCalledOnce());
  expect(dispatch).toHaveBeenCalledWith('skills.bindSource',{inspectionId:'inspection',subpath:'skills/json-canvas',skillId:'canvas'});
  expect(refresh).toHaveBeenCalledOnce();
});

it('leaves different same-name directories selectable instead of guessing a source',async()=>{
  const dispatch=vi.spyOn(api,'dispatch').mockImplementation(async (...[method]) =>method==='repositories.list'?{repositories:[]} as never:method==='sources.inspect'?{inspectionId:'inspection',source:{kind:'git',locator:'https://github.com/example/repo'},candidates:[{name:'json-canvas',subpath:'skills/json-canvas'},{name:'json-canvas',subpath:'plugins/canvas/skills/json-canvas'}]} as never:{} as never);
  render(<AddSkillDialog deployments={[]} bindSkill={skill} refresh={vi.fn()} onClose={vi.fn()}/>);
  await userEvent.type(screen.getByLabelText('Skill 来源'),'example/repo');
  await userEvent.click(screen.getByRole('button',{name:'查找来源'}));
  expect(await screen.findAllByRole('radio')).toHaveLength(2);
  expect(screen.getAllByRole('radio').every(r=>!(r as HTMLInputElement).checked)).toBe(true);
  expect(screen.getByRole('button',{name:'保存来源'})).toBeDisabled();
  expect(dispatch.mock.calls.some(([m])=>m==='skills.bindSource')).toBe(false);
});

const inspection={inspectionId:'stage',source:{kind:'git',locator:'https://github.com/example/repo'},candidates:[{name:'One',description:'first',subpath:'one'},{name:'Two',description:'second',subpath:'two'}]};
it('adds multiple selected Skills, keeps completed items on failure, and retries only failed items',async()=>{
  let failed=false;const dispatch=vi.spyOn(api,'dispatch').mockImplementation(async (...[method,args]) =>{
    if(method==='repositories.list')return {repositories:[]} as never;
    if(method==='sources.inspect')return structuredClone(inspection) as never;
    if(method==='skills.add'&&args?.subpath==='two'&&!failed){failed=true;throw Error('second failed');}
    return {} as never;
  });
  const close=vi.fn();render(<AddSkillDialog deployments={[]} refresh={vi.fn().mockResolvedValue(undefined)} onClose={close}/>);
  await userEvent.type(screen.getByLabelText('Skill 来源'),'example/repo');await userEvent.click(screen.getByRole('button',{name:'查找 Skill'}));
  await userEvent.click(await screen.findByLabelText('全选找到的 Skill'));await userEvent.click(screen.getByRole('button',{name:'添加到 Skill 库 (2)'}));
  expect(await screen.findByRole('alert')).toHaveTextContent('second failed');expect(close).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole('button',{name:'添加到 Skill 库 (1)'}));await waitFor(()=>expect(close).toHaveBeenCalledTimes(1));
  expect(dispatch.mock.calls.flatMap(([m,args])=>m==='skills.add'?[args.subpath]:[])).toEqual(['one','two','two']);
});
it('releases source inspection that finishes after the dialog unmounts',async()=>{
  let resolve!:(value:typeof inspection)=>void;const pending=new Promise<typeof inspection>(r=>{resolve=r;});
  const dispatch=vi.spyOn(api,'dispatch').mockImplementation(async (...[method]) =>method==='sources.inspect'?await pending as never:{repositories:[]} as never);
  const view=render(<AddSkillDialog deployments={[]} refresh={vi.fn()} onClose={vi.fn()}/>);
  await userEvent.type(screen.getByLabelText('Skill 来源'),'example/repo');await userEvent.click(screen.getByRole('button',{name:'查找 Skill'}));view.unmount();
  await act(async()=>resolve(structuredClone(inspection)));expect(dispatch).toHaveBeenCalledWith('sources.release',{inspectionId:'stage'});
});
