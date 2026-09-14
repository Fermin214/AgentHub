import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import * as api from './api';
import { useSkillUpdates } from './useSkillUpdates';
import type { Snapshot,UpdateCheck } from './types';
beforeEach(()=>vi.restoreAllMocks());
it('limits a batch to three checks, refills slots as they finish, and releases shared sources after failure',async()=>{
  const data:Snapshot={dataScope:'fixture',prompts:[],deployments:[],projects:[],settings:{scanRoots:[],executables:{codex:'',claude:'',dsh:''}},operations:[],skills:Array.from({length:5},(_,i)=>({id:String(i),name:'Skill '+i,path:'C:/library/'+i,source:{kind:'git',locator:'https://github.com/example/repo'},description:'',tags:[],favorite:false,createdAt:'',updatedAt:''}))};
  const completions=new Map<string,{resolve:(v:UpdateCheck)=>void;reject:(e:Error)=>void}>();
  const dispatch=vi.spyOn(api,'dispatch').mockImplementation(async (...[method,args]) =>{
    if(method==='skills.checkBatch.start')return {batchId:'batch',concurrency:3} as never;
    if(method==='skills.check')return await new Promise<UpdateCheck>((resolve,reject)=>completions.set(String(args?.skillId),{resolve,reject})) as never;
    return {} as never;
  });
  const hook=renderHook(()=>useSkillUpdates(data,vi.fn().mockResolvedValue(undefined),vi.fn()));
  let batch!:Promise<void>;act(()=>{batch=hook.result.current.checkAll();});
  await waitFor(()=>expect(completions.size).toBe(3));expect(hook.result.current.checkingIds).toHaveLength(3);
  await act(async()=>completions.get('1')!.resolve({skillId:'1',name:'Skill 1',status:'current',message:'一致'}));
  await waitFor(()=>expect(completions.size).toBe(4));expect(hook.result.current.checkProgress).toBe('1 / 5');
  await act(async()=>completions.get('0')!.reject(Error('network failure')));
  await waitFor(()=>expect(completions.size).toBe(5));
  await act(async()=>{for(const id of ['2','3','4'])completions.get(id)!.resolve({skillId:id,name:'Skill '+id,status:'current',message:'一致'});await batch;});
  expect(hook.result.current.error).toContain('1 个 Skill 检查失败（Skill 0）');expect(hook.result.current.checkingIds).toEqual([]);
  expect(dispatch.mock.calls.filter(([m])=>m==='skills.check')).toHaveLength(5);
  expect(dispatch).toHaveBeenLastCalledWith('skills.checkBatch.finish',{batchId:'batch'});
});
