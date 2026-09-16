import { beforeEach, expect, it, vi } from 'vitest';
import { act, render, screen, within, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import * as api from './api';
import App from './App';
import type { Snapshot, UpdateCheck } from './types';
const snapshot:Snapshot={dataScope:'fixture',prompts:[],skills:[],deployments:[],projects:[],settings:{scanRoots:[],executables:{codex:'',claude:'',dsh:''}},operations:[]};
beforeEach(()=>{vi.restoreAllMocks();vi.spyOn(api,'getSnapshot').mockResolvedValue(structuredClone(snapshot));vi.spyOn(api,'isTauriRuntime').mockReturnValue(false);});
it('keeps other Skills in the list when metadata.save returns only the saved record',async()=>{
  const write=structuredClone(snapshot);
  const skills=[{id:'s',name:'Writer',path:'C:/library/writer',source:{kind:'unknown' as const,locator:''},description:'',tags:[],favorite:false,createdAt:'',updatedAt:''},{id:'s2',name:'Second',path:'C:/library/second',source:{kind:'unknown' as const,locator:''},description:'',tags:[],favorite:false,createdAt:'',updatedAt:''}];
  write.skills=structuredClone(skills);
  // A slow reload keeps the UI on the save result, which is where a partial
  // payload used to drop every other row.
  let finishReload!:(value:Snapshot)=>void;
  const slowReload=new Promise<Snapshot>(resolve=>{finishReload=resolve;});
  vi.mocked(api.getSnapshot).mockResolvedValueOnce(structuredClone(write)).mockReturnValue(slowReload);
  const dispatch=vi.spyOn(api,'dispatch').mockImplementation(async (...[method,args])=>{
    if(method==='targets.list')return {targets:[]} as never;
    if(method==='bookmarks.sync')return {items:[]} as never;
    if(method==='skills.metadata.save'){
      write.skills=write.skills.map(skill=>args.ids.includes(skill.id)?{...skill,...(args.favorite===undefined?{}:{favorite:args.favorite})}:skill);
      return {skills:structuredClone(write.skills.filter(skill=>args.ids.includes(skill.id)))} as never;
    }
    return {} as never;
  });
  render(<App/>);
  await userEvent.click(await screen.findByRole('button',{name:'Skill 内容与安装位置'}));
  expect(await screen.findByRole('button',{name:'Second'})).toBeVisible();
  await userEvent.click(screen.getByRole('button',{name:'收藏 Writer'}));
  await waitFor(()=>expect(screen.getByRole('button',{name:'取消收藏 Writer'})).toBeEnabled());
  expect(screen.getByRole('button',{name:'Second'})).toBeInTheDocument();
  expect(dispatch).toHaveBeenCalledWith('skills.metadata.save',{ids:['s'],favorite:true});
  await act(async()=>finishReload(structuredClone(write)));
  expect(screen.getByRole('button',{name:'Second'})).toBeInTheDocument();
  expect(screen.getByRole('button',{name:'取消收藏 Writer'})).toBeEnabled();
});
it('keeps an ongoing check across navigation and restores its persisted update button after reopening',async()=>{
  let resolve!:(value:UpdateCheck)=>void;
  const pending=new Promise<UpdateCheck>(r=>{resolve=r;});
  const data:Snapshot={...snapshot,skills:[{id:'kami',name:'Kami',path:'C:/library/kami',source:{kind:'git',locator:'https://github.com/tw93/Kami'},description:'',tags:[],favorite:false,createdAt:'',updatedAt:''}],skillUpdates:[]};
  const result:UpdateCheck={skillId:'kami',name:'Kami',status:'available',message:'有更新可用',checkedAt:'2026-09-13T06:00:00Z',checkId:'check',locations:[{id:'library',label:'Skill 库',path:'C:/library/kami',agents:[],exists:true,differences:[{path:'SKILL.md',change:'modified'}]}]};
  vi.mocked(api.getSnapshot).mockImplementation(async()=>structuredClone(data));
  const dispatch=vi.spyOn(api,'dispatch').mockImplementation(async (...[method]) =>method==='skills.check'?await pending as never:method==='targets.list'?{targets:[]} as never:method==='bookmarks.sync'?{items:[]} as never:{} as never);
  const view=render(<App/>);
  await userEvent.click(await screen.findByRole('button',{name:'Skill 内容与安装位置'}));
  await userEvent.click(screen.getByRole('button',{name:'检查更新'}));
  await userEvent.click(screen.getByRole('button',{name:'项目 本机目录与仓库收藏'}));
  data.skillUpdates=[result];await act(async()=>resolve(result));
  await userEvent.click(screen.getByRole('button',{name:'Skill 内容与安装位置'}));
  expect(await screen.findByRole('button',{name:'更新'})).toBeVisible();
  expect(screen.queryByRole('button',{name:'查看变化'})).not.toBeInTheDocument();
  expect(screen.queryByRole('button',{name:'检查更新'})).not.toBeInTheDocument();
  view.unmount();render(<App/>);
  await userEvent.click(await screen.findByRole('button',{name:'Skill 内容与安装位置'}));
  expect(await screen.findByRole('button',{name:'更新'})).toBeVisible();
  expect(dispatch.mock.calls.filter(([m])=>m==='skills.check')).toHaveLength(1);
});
it('opens Prompt first and preserves plain-text create, copy and edit interactions',async()=>{
  vi.spyOn(api,'savePrompt').mockImplementation(async (...[p]) =>({...p,id:'p'}));const copy=vi.spyOn(api,'copyText').mockResolvedValue();
  render(<App/>);expect(await screen.findByRole('heading',{name:'Prompts',level:1})).toBeVisible();
  const navigation=within(screen.getByRole('navigation',{name:'主导航'}));expect(navigation.getAllByRole('button').map(b=>b.querySelector('strong')?.textContent)).toEqual(['Prompts','Skill','项目','设置']);
  await userEvent.click(screen.getByRole('button',{name:'添加'}));
  await userEvent.type(screen.getByLabelText('正文'),'原文第一行\n第二行');await userEvent.type(screen.getByLabelText('标签，用逗号分隔'),'写作');
  await userEvent.click(screen.getByRole('button',{name:'保存 Prompt'}));await userEvent.click(await screen.findByRole('button',{name:'查看 原文第一行'}));
  expect(copy).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole('button',{name:'复制正文'}));
  expect(copy).toHaveBeenCalledWith('原文第一行\n第二行');
  await userEvent.click(screen.getAllByRole('button',{name:'关闭'})[0]);
  await userEvent.click(screen.getByRole('button',{name:'编辑 Prompt'}));expect(screen.getByLabelText('正文')).toHaveValue('原文第一行\n第二行');
});
it('exports the exact content returned by the core',async()=>{
  vi.spyOn(api,'exportPrompts').mockResolvedValue({content:'EXACT',filename:'prompts.json'});const saveFile=vi.spyOn(api,'saveExportFile').mockResolvedValue();
  render(<App/>);await userEvent.click(await screen.findByRole('button',{name:'导出'}));await userEvent.click(screen.getByRole('button',{name:'导出文件'}));await waitFor(()=>expect(saveFile).toHaveBeenCalledWith('EXACT','prompts.json'));
});
it('keeps Prompt export and settings accessible during recovery and retries without dismissing diagnostics',async()=>{
  const restricted:Snapshot={...snapshot,recovery:{status:'restricted',code:'RECOVERY_REQUIRED',detail:'文件占用',issues:[{id:'plan-1',state:'committed',paths:['C:/affected/skill']}]}};
  vi.mocked(api.getSnapshot).mockResolvedValue(restricted);
  vi.spyOn(api,'exportPrompts').mockResolvedValue({content:'SAFE',filename:'prompts.json'});
  const save=vi.spyOn(api,'saveExportFile').mockResolvedValue();
  vi.spyOn(api,'listBackups').mockResolvedValue({backups:[]});
  vi.spyOn(api,'dispatch').mockResolvedValue({targets:[]} as never);
  render(<App/>);
  expect(await screen.findByRole('heading',{name:'部分文件需要恢复，Skill 写入已暂停'})).toBeVisible();
  await userEvent.click(screen.getByRole('button',{name:'导出'}));
  await userEvent.click(screen.getByRole('button',{name:'导出文件'}));
  await waitFor(()=>expect(save).toHaveBeenCalledWith('SAFE','prompts.json'));
  await userEvent.click(screen.getByRole('button',{name:'设置 Agent 与本地偏好'}));
  expect(await screen.findByRole('heading',{name:'设置',level:1})).toBeVisible();
  await userEvent.click(screen.getByRole('button',{name:'重试恢复'}));
  expect(screen.getByRole('alert')).toHaveTextContent('部分文件需要恢复');
  vi.mocked(api.getSnapshot).mockResolvedValue({...snapshot,recovery:{status:'ready',issues:[]}});
  await userEvent.click(screen.getByRole('button',{name:'重试恢复'}));
  await waitFor(()=>expect(screen.queryByText('部分文件需要恢复，Skill 写入已暂停')).not.toBeInTheDocument());
});
it('keeps a failed Prompt save open and surfaces the error',async()=>{
  vi.spyOn(api,'savePrompt').mockRejectedValue(Error('磁盘不可写'));render(<App/>);await userEvent.click(await screen.findByRole('button',{name:'添加'}));await userEvent.type(screen.getByLabelText('正文'),'不要丢失');await userEvent.click(screen.getByRole('button',{name:'保存 Prompt'}));expect(await screen.findByText('磁盘不可写')).toBeVisible();expect(screen.getByLabelText('正文')).toHaveValue('不要丢失');
});
