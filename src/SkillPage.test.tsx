import { beforeEach, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import * as api from './api';
import { SkillPage as SkillPageView } from './SkillPage';
import { useSkillUpdates } from './useSkillUpdates';
import type { ComponentProps } from 'react';
function SkillPage(props:Omit<ComponentProps<typeof SkillPageView>,'controller'>){const controller=useSkillUpdates(props.snapshot,props.refresh,props.notify);return <SkillPageView {...props} controller={controller}/>;}
import type { Skill, SkillChangePlan, Snapshot } from './types';
const source={kind:'git' as const,locator:'https://github.com/example/repo'};
const snapshot:Snapshot={dataScope:'fixture',prompts:[],skills:[{id:'s',name:'Writer',description:'writing',path:'C:/library/writer',source,tags:[],favorite:false,createdAt:'',updatedAt:''}],deployments:[],projects:[],settings:{scanRoots:[],executables:{codex:'',claude:'',dsh:''}},operations:[]};
const targets=[{id:'codex',name:'Codex',enabled:true,globalPath:'C:/agent/skills',projectPath:'.agents/skills'}];
const plan:SkillChangePlan={id:'plan',skillId:'s',action:'install',summary:'安装 Writer',canExecute:true,createdAt:'',locations:[{id:'loc',path:'C:/agent/skills/writer',label:'Codex',agents:['codex'],exists:false,differences:[]}]};
beforeEach(()=>vi.restoreAllMocks());
it('opens prepared differences immediately while preferences load, without checking the source again',async()=>{
  let finishPolicy!:(v:unknown)=>void;
  const policy=new Promise(resolve=>{finishPolicy=resolve;});
  const locations=[{id:'library',path:'C:/library/writer',label:'Skill 库',agents:[],exists:true,differences:[{path:'SKILL.md',change:'modified'}]}];
  const dispatch=vi.spyOn(api,'dispatch').mockImplementation(async (...[method]) =>method==='targets.list'?{targets} as never:method==='maintenance.get'?policy as never:{} as never);
  render(<SkillPage snapshot={{...snapshot,skillUpdates:[{skillId:'s',name:'Writer',status:'available',message:'来源有变化',checkId:'saved',locations}]}} refresh={vi.fn()} notify={vi.fn()} onProject={vi.fn()}/>);
  await userEvent.click(await screen.findByRole('button',{name:'更新'}));
  const dialog=within(screen.getByRole('dialog'));
  expect(dialog.getByRole('button',{name:'SKILL.md'})).toBeVisible();
  expect(dialog.queryByText('正在准备更新内容…')).not.toBeInTheDocument();
  expect(dialog.getByRole('button',{name:'确认更新'})).toBeDisabled();
  expect(dispatch.mock.calls.some(([m])=>m==='skills.check')).toBe(false);
  await act(async()=>finishPolicy({retainUpdateBackup:false}));
  expect(dialog.getByLabelText('更新前保留备份')).not.toBeChecked();
  expect(dialog.getByRole('button',{name:'确认更新'})).toBeEnabled();
});
it('does not write when the final file validation blocks an update and cancels its plan',async()=>{
  const locations=[{id:'library',path:'C:/library/writer',label:'Skill 库',agents:[],exists:true,differences:[{path:'SKILL.md',change:'modified'}]}];
  const dispatch=vi.spyOn(api,'dispatch').mockImplementation(async (...[method]) =>method==='targets.list'?{targets} as never:method==='skills.update.preview'?{...plan,action:'update',canExecute:false,blockedReason:'检查之后文件已变化，请重新检查'} as never:{} as never);
  render(<SkillPage snapshot={{...snapshot,skillUpdates:[{skillId:'s',name:'Writer',status:'available',message:'来源有变化',checkId:'saved',locations}]}} refresh={vi.fn()} notify={vi.fn()} onProject={vi.fn()}/>);
  await userEvent.click(await screen.findByRole('button',{name:'更新'}));
  await userEvent.click(screen.getByRole('button',{name:'确认更新'}));
  expect(await screen.findByRole('alert')).toHaveTextContent('检查之后文件已变化');
  expect(dispatch.mock.calls.some(([m])=>m==='skills.update')).toBe(false);
  expect(dispatch).toHaveBeenCalledWith('skills.cancel',{planId:'plan'});
});
it('removes a bound source only after confirmation',async()=>{
  const dispatch=vi.spyOn(api,'dispatch').mockImplementation(async (...[method]) =>method==='targets.list'?{targets} as never:{} as never);
  const refresh=vi.fn().mockResolvedValue(undefined);
  render(<SkillPage snapshot={snapshot} refresh={refresh} notify={vi.fn()} onProject={vi.fn()}/>);
  expect(screen.queryByRole('button',{name:'设置来源'})).not.toBeInTheDocument();
  await userEvent.click(screen.getByRole('button',{name:'删除来源'}));
  expect(dispatch.mock.calls.some(([m])=>m==='skills.unbindSource')).toBe(false);
  await userEvent.click(screen.getByRole('button',{name:'确认删除来源'}));
  await waitFor(()=>expect(refresh).toHaveBeenCalledOnce());
  expect(dispatch).toHaveBeenCalledWith('skills.unbindSource',{skillId:'s'});
});
it('keeps other Skills and repository controls usable during a single check and reuses that check in the batch',async()=>{
  let resolve!:(v:unknown)=>void;
  const pending=new Promise(r=>{resolve=r;});
  const dispatch=vi.spyOn(api,'dispatch').mockImplementation(async (...[method,args]) =>{
    if(method==='targets.list')return {targets} as never;
    if(method==='skills.checkBatch.start')return {batchId:'batch',concurrency:3} as never;
    if(method==='skills.check')return (args?.skillId==='s'?pending:{message:'Second done'}) as never;
    return {} as never;
  });
  render(<SkillPage snapshot={{...snapshot,skills:[snapshot.skills[0],{...snapshot.skills[0],id:'s2',name:'Second'}]}} refresh={vi.fn()} notify={vi.fn()} onProject={vi.fn()}/>);
  await userEvent.click(screen.getAllByRole('button',{name:'检查更新'})[0]);
  expect(screen.getByRole('button',{name:'检查中…'})).toBeDisabled();
  expect(screen.getByRole('button',{name:'检查更新'})).toBeEnabled();
  expect(screen.getByRole('button',{name:'Skill 仓库'})).toBeEnabled();
  expect(screen.getByRole('button',{name:'检查全部更新'})).toBeEnabled();
  await userEvent.click(screen.getByRole('button',{name:'检查全部更新'}));
  await act(async()=>resolve({message:'First done'}));
  await screen.findByText('Second done');
  expect(dispatch.mock.calls.flatMap(([m,args])=>m==='skills.check'?[args.skillId]:[])).toEqual(['s','s2']);
});
// Models the real transport: `skills.metadata.save` returns only the records it
// touched, and a snapshot reload afterwards returns the whole committed store.
// The mock also drives the snapshot prop like App does, because that reload result
// is what confirms a committed favorite value.
let snapshotState:Snapshot=snapshot;
const favoriteDispatch=(skills:Skill[]=twoSkillList())=>{
  snapshotState={...snapshot,skills:structuredClone(skills)};
  const store=structuredClone(skills);
  const saves:Array<{ids:string[];favorite?:boolean;tags?:string[];resolve:(v:unknown)=>void;reject:(e:unknown)=>void}>=[];const calls:string[]=[];
  const dispatch=vi.spyOn(api,'dispatch').mockImplementation((...[method,args])=>{
    if(method==='targets.list')return Promise.resolve({targets} as never);
    if(method==='skills.metadata.save'){
      let resolve!:(v:unknown)=>void,reject!:(e:unknown)=>void;const pending=new Promise((ok,fail)=>{resolve=ok;reject=fail;});
      calls.push(...args.ids);saves.push({ids:args.ids,favorite:args.favorite,tags:args.tags,resolve,reject});return pending as never;
    }
    return Promise.resolve({} as never);
  });
  // A reload reports the committed store, exactly like the real snapshot.
  const refresh=vi.fn().mockImplementation(async()=>{snapshotState={...snapshotState,skills:structuredClone(store)};});
  // Applies the patch to the store and resolves with only the requested records.
  const settle=(index:number,respond?:(skill:Skill,favorite:boolean)=>Partial<Skill>)=>{
    const save=saves[index];
    store.forEach((skill,position)=>{if(!save.ids.includes(skill.id))return;const next={...skill};if(save.favorite!==undefined)next.favorite=save.favorite;if(save.tags!==undefined)next.tags=save.tags;store[position]=respond?{...next,...respond(skill,!!save.favorite)}:next;});
    save.resolve({skills:structuredClone(store.filter(skill=>save.ids.includes(skill.id)))});
  };
  // Renders through a stateful snapshot prop so a completed reload confirms values.
  type HarnessProps={notify:ComponentProps<typeof SkillPageView>['notify'];onProject:ComponentProps<typeof SkillPageView>['onProject'];refresh?:ComponentProps<typeof SkillPageView>['refresh'];onSkills?:(skills:Skill[])=>void;projectId?:string};
  const Harness=({refresh:override,...rest}:HarnessProps)=>{
    const activeRefresh=override??refresh;
    const controller=useSkillUpdates(snapshotState,activeRefresh,rest.notify);
    return <SkillPageView {...rest} snapshot={snapshotState} refresh={activeRefresh} controller={controller}/>;
  };
  return {dispatch,saves,calls,refresh,store,settle,Harness};
};
function twoSkillList():Skill[]{return [{...snapshot.skills[0]},{...snapshot.skills[0],id:'s2',name:'Second'}];}const twoSkills={...snapshot,skills:twoSkillList()};
it('saves a favorite once per click and leaves every other control in its normal state',async()=>{
  const {calls,settle,Harness}=favoriteDispatch();
  render(<Harness notify={vi.fn()} onProject={vi.fn()}/>);
  await screen.findByRole('button',{name:'收藏 Writer'});
  const star=screen.getByRole('button',{name:'收藏 Writer'});
  star.focus();
  const before=screen.getAllByRole('button',{name:'从库删除'}).map(button=>button.hasAttribute('disabled'));
  fireEvent.click(star);fireEvent.click(star);
  expect(star).toBeDisabled();expect(star).toHaveFocus();expect(star).toHaveAttribute('aria-label','正在保存 Writer 的收藏…');
  expect(calls).toEqual(['s']);
  // B-1: a favorite save must not disable or dim any other control.
  expect(screen.getAllByRole('button',{name:'从库删除'}).map(button=>button.hasAttribute('disabled'))).toEqual(before);
  expect(screen.getAllByRole('button',{name:'从库删除'}).every(button=>!button.hasAttribute('disabled'))).toBe(true);
  expect(screen.getByRole('button',{name:'收藏 Second'})).toBeEnabled();
  expect(screen.getByRole('button',{name:'Skill 仓库'})).toBeEnabled();
  expect(screen.getByRole('button',{name:'添加 Skill'})).toBeEnabled();
  const agentButtons=document.querySelectorAll<HTMLButtonElement>('.skill-sync__agent');
  expect(agentButtons.length).toBeGreaterThan(0);
  expect([...agentButtons].every(button=>!button.disabled)).toBe(true);
  expect([...agentButtons].every(button=>button.style.opacity!=='0.5')).toBe(true);
  await act(async()=>settle(0));
  expect(calls).toEqual(['s']);
  expect(screen.getByRole('button',{name:'取消收藏 Writer'})).toBeEnabled();
});
it('applies the authoritative save result and keeps it until the snapshot confirms it',async()=>{
  const onSkills=vi.fn();
  const frozen=vi.fn().mockImplementation(async()=>{});
  const {settle,Harness}=favoriteDispatch();
  render(<Harness refresh={frozen} notify={vi.fn()} onProject={vi.fn()} onSkills={onSkills}/>);
  fireEvent.click(await screen.findByRole('button',{name:'收藏 Writer'}));
  await act(async()=>settle(0));
  // The shell receives only this save's records, so a merging caller cannot revert
  // any row another concurrent save already committed.
  expect(onSkills.mock.calls[0][0]).toEqual([expect.objectContaining({id:'s',favorite:true})]);
  // The shell must not replace tags or other fields from a favorite-only response.
  expect(onSkills.mock.calls[0][1]).toEqual(['favorite']);
  expect(frozen).toHaveBeenCalled();
  // A stale reload must not drop the committed value back to the old record.
  expect(screen.getByRole('button',{name:'取消收藏 Writer'})).toBeEnabled();
});
it('does not re-save the old value when the snapshot reload is slow',async()=>{
  let finishRefresh!:(v:unknown)=>void;
  const {settle,saves,Harness}=favoriteDispatch();
  const slowRefresh=vi.fn().mockImplementation(()=>new Promise(resolve=>{finishRefresh=resolve;}));
  render(<Harness refresh={slowRefresh} notify={vi.fn()} onProject={vi.fn()}/>);
  fireEvent.click(await screen.findByRole('button',{name:'收藏 Writer'}));
  await act(async()=>settle(0));
  // While the reload is pending the committed value is shown, so a second click
  // acts on the new state instead of repeating a save of the stale record.
  fireEvent.click(screen.getByRole('button',{name:'取消收藏 Writer'}));
  expect(saves).toHaveLength(2);
  expect(saves[1].favorite).toBe(false);
  await act(async()=>settle(1));
  await act(async()=>{finishRefresh({});});
  expect(screen.getByRole('button',{name:'收藏 Writer'})).toBeEnabled();
});
it('round-trips favorite, unfavorite and favorite again after confirmation',async()=>{
  const {saves,store,settle,Harness}=favoriteDispatch();
  render(<Harness notify={vi.fn()} onProject={vi.fn()}/>);
  fireEvent.click(await screen.findByRole('button',{name:'收藏 Writer'}));
  await act(async()=>settle(0));
  const undo=screen.getByRole('button',{name:'取消收藏 Writer'});
  expect(undo).toBeEnabled();
  fireEvent.click(undo);
  await act(async()=>settle(1));
  expect(screen.getByRole('button',{name:'收藏 Writer'})).toBeEnabled();
  fireEvent.click(screen.getByRole('button',{name:'收藏 Writer'}));
  await act(async()=>settle(2));
  // Every step after a confirmed save must still be able to act on the new state.
  expect(saves.map(save=>save.favorite)).toEqual([true,false,true]);
  expect(store.find(skill=>skill.id==='s')?.favorite).toBe(true);
});
it('allows favoriting after a tags save and its snapshot confirmation completed',async()=>{
  const {saves,settle,Harness}=favoriteDispatch();
  render(<Harness notify={vi.fn()} onProject={vi.fn()}/>);
  const writer=document.querySelectorAll<HTMLElement>('.library-row')[0];
  expect(writer).toBeDefined();
  fireEvent.click(within(writer).getByRole('button',{name:'标签'}));
  fireEvent.click(screen.getByRole('button',{name:'保存标签'}));
  await act(async()=>settle(0));
  expect(saves[0].tags).toEqual([]);
  expect(saves[0].favorite).toBeUndefined();
  fireEvent.click(screen.getByRole('button',{name:'收藏 Writer'}));
  await act(async()=>settle(1));
  // A tags save must not leave a favorite record behind that blocks the star.
  expect(saves.filter(save=>save.favorite!==undefined)).toHaveLength(1);
  expect(saves[1]).toMatchObject({ids:['s'],favorite:true});
});
it('retires a committed favorite once a new snapshot arrives and then follows new values',async()=>{
  // A container with real React state: passing a new snapshot prop is what actually
  // confirms a committed value, so the override lifecycle is observable here.
  const baseProps={notify:vi.fn(),onProject:vi.fn(),refresh:vi.fn().mockResolvedValue(undefined)};
  const Container=({next}:{next:Skill[]})=>{
    const current={...snapshot,skills:next};
    const controller=useSkillUpdates(current,baseProps.refresh,baseProps.notify);
    return <SkillPageView {...baseProps} snapshot={current} controller={controller}/>;
  };
  const committed=twoSkillList();committed[0]={...committed[0],favorite:true};
  const reverted=twoSkillList();
  const {settle}=favoriteDispatch();
  const view=render(<Container next={twoSkillList()}/>);
  fireEvent.click(await screen.findByRole('button',{name:'收藏 Writer'}));
  await act(async()=>settle(0));
  expect(screen.getByRole('button',{name:'取消收藏 Writer'})).toBeEnabled();
  // A snapshot that carries the committed value retires the local override.
  await act(async()=>view.rerender(<Container next={committed}/>));
  expect(screen.getByRole('button',{name:'取消收藏 Writer'})).toBeEnabled();
  // A later authoritative snapshot that changes it must now be able to take over.
  await act(async()=>view.rerender(<Container next={reverted}/>));
  expect(screen.getByRole('button',{name:'收藏 Writer'})).toBeEnabled();
});
it('reports a failed snapshot reload as saved-but-stale instead of unsaved',async()=>{
  const notify=vi.fn();
  const {settle}=favoriteDispatch();
  const refresh=vi.fn().mockRejectedValue(new Error('快照读取失败'));
  render(<SkillPage snapshot={snapshot} refresh={refresh} notify={notify} onProject={vi.fn()}/>);
  fireEvent.click(await screen.findByRole('button',{name:'收藏 Writer'}));
  await act(async()=>settle(0));
  expect(notify).toHaveBeenCalledWith('收藏已保存，但刷新 Skill 列表失败。显示可能不是最新。','error');
  expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  expect(screen.getByRole('button',{name:'取消收藏 Writer'})).toBeEnabled();
});
it('applies the committed favorite to the favorites-only filter before the snapshot returns',async()=>{
  let finishRefresh!:(v:unknown)=>void;
  const refresh=vi.fn().mockImplementation(()=>new Promise(resolve=>{finishRefresh=resolve;}));
  const onSkills=vi.fn();
  const saves:Array<{resolve:(v:unknown)=>void}>=[];
  vi.spyOn(api,'dispatch').mockImplementation((...[method,args])=>{
    if(method==='targets.list')return Promise.resolve({targets} as never);
    if(method==='skills.metadata.save'){let resolve!:(v:unknown)=>void;const pending=new Promise(ok=>{resolve=ok;});saves.push({resolve});void args;return pending as never;}
    return Promise.resolve({} as never);
  });
  const skills=[{...snapshot.skills[0],favorite:true},{...snapshot.skills[0],id:'s2',name:'Second',favorite:false}];
  const view=render(<SkillPage snapshot={{...twoSkills,skills}} refresh={refresh} notify={vi.fn()} onProject={vi.fn()} onSkills={onSkills}/>);
  await userEvent.click(screen.getByRole('checkbox',{name:'仅看收藏'}));
  expect(screen.getByRole('button',{name:'Writer'})).toBeVisible();
  expect(screen.queryByRole('button',{name:'Second'})).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole('button',{name:'取消收藏 Writer'}));
  await act(async()=>saves[0].resolve({skills:skills.map(skill=>skill.id==='s'?{...skill,favorite:false}:skill)}));
  const patched=onSkills.mock.calls[0][0] as Skill[];
  view.rerender(<SkillPage snapshot={{...twoSkills,skills:patched}} refresh={refresh} notify={vi.fn()} onProject={vi.fn()} onSkills={onSkills}/>);
  // The unfavorited row leaves the favorites-only list without waiting for the snapshot.
  expect(screen.queryByRole('button',{name:'Writer'})).not.toBeInTheDocument();
  await act(async()=>finishRefresh({}));
});
it('keeps concurrent favorite saves independent when they finish out of order',async()=>{
  const {saves,settle}=favoriteDispatch();
  render(<SkillPage snapshot={twoSkills} refresh={vi.fn().mockResolvedValue(undefined)} notify={vi.fn()} onProject={vi.fn()}/>);
  await act(async()=>{fireEvent.click(screen.getByRole('button',{name:'收藏 Writer'}));fireEvent.click(screen.getByRole('button',{name:'收藏 Second'}));});
  expect(saves.map(save=>save.ids)).toEqual([['s'],['s2']]);
  expect(screen.getByRole('button',{name:'正在保存 Writer 的收藏…'})).toBeDisabled();
  expect(screen.getByRole('button',{name:'正在保存 Second 的收藏…'})).toBeDisabled();
  await act(async()=>settle(1));
  expect(screen.getByRole('button',{name:'取消收藏 Second'})).toBeEnabled();
  expect(screen.getByRole('button',{name:'正在保存 Writer 的收藏…'})).toBeDisabled();
  await act(async()=>settle(0));
  expect(screen.getByRole('button',{name:'取消收藏 Writer'})).toBeEnabled();
});
it('keeps an in-flight review protection while a favorite save is also running',async()=>{
  let finishPreview!:(v:unknown)=>void;
  const preview=new Promise(resolve=>{finishPreview=resolve;});
  const saves:Array<{ids:string[];favorite?:boolean;resolve:(v:unknown)=>void}>=[];
  vi.spyOn(api,'dispatch').mockImplementation((...[method,args])=>{
    if(method==='targets.list')return Promise.resolve({targets} as never);
    if(method==='skills.install.preview')return preview as never;
    if(method==='skills.metadata.save'){
      let resolve!:(v:unknown)=>void;const pending=new Promise(ok=>{resolve=ok;});saves.push({ids:args.ids,favorite:args.favorite,resolve});return pending as never;
    }
    return Promise.resolve({} as never);
  });
  render(<SkillPage snapshot={snapshot} refresh={vi.fn().mockResolvedValue(undefined)} notify={vi.fn()} onProject={vi.fn()}/>);
  await screen.findByRole('button',{name:'收藏 Writer'});
  const install=screen.getByRole('button',{name:'安装 Writer 到 Codex'});
  fireEvent.click(install);
  expect(install).toBeDisabled();
  const star=screen.getByRole('button',{name:'收藏 Writer'});
  expect(star).toBeEnabled();
  fireEvent.click(star);
  expect(saves).toHaveLength(1);
  // B-2: the favorite resolving must not release the review operation's protection.
  await act(async()=>saves[0].resolve({skills:[{...snapshot.skills[0],favorite:true}]}));
  expect(install).toBeDisabled();
  await act(async()=>finishPreview({...plan,action:'install',canExecute:true}));
  expect(await screen.findByRole('dialog')).toBeVisible();
});
it('does not release an unfinished favorite when another operation ends',async()=>{
  let finishPreview!:(v:unknown)=>void;
  const preview=new Promise(resolve=>{finishPreview=resolve;});
  const saves:Array<{ids:string[];favorite?:boolean;resolve:(v:unknown)=>void}>=[];
  vi.spyOn(api,'dispatch').mockImplementation((...[method,args])=>{
    if(method==='targets.list')return Promise.resolve({targets} as never);
    if(method==='skills.install.preview')return preview as never;
    if(method==='skills.metadata.save'){let resolve!:(v:unknown)=>void;const pending=new Promise(ok=>{resolve=ok;});saves.push({ids:args.ids,favorite:args.favorite,resolve});return pending as never;}
    return Promise.resolve({} as never);
  });
  render(<SkillPage snapshot={snapshot} refresh={vi.fn().mockResolvedValue(undefined)} notify={vi.fn()} onProject={vi.fn()}/>);
  await screen.findByRole('button',{name:'收藏 Writer'});
  fireEvent.click(screen.getByRole('button',{name:'安装 Writer 到 Codex'}));
  fireEvent.click(screen.getByRole('button',{name:'收藏 Writer'}));
  expect(saves).toHaveLength(1);
  expect(screen.getByRole('button',{name:'正在保存 Writer 的收藏…'})).toBeDisabled();
  await act(async()=>finishPreview({...plan,action:'install',canExecute:true}));
  // The review finishing must not clear the favorite's own waiting state. The
  // review dialog now covers the list, so check the star through the DOM directly.
  const pendingStar=document.querySelector<HTMLButtonElement>('button[data-favorite-pending="true"]');
  expect(pendingStar).not.toBeNull();
  expect(pendingStar!.disabled).toBe(true);
  await act(async()=>saves[0].resolve({skills:[{...snapshot.skills[0],favorite:true}]}));
  const resolvedStar=document.querySelector<HTMLButtonElement>('button[data-favorite-pending="true"]');
  expect(resolvedStar).toBeNull();
  expect(document.querySelector<HTMLButtonElement>('button[aria-label="取消收藏 Writer"]')).not.toBeNull();
});
it('recovers a rejected favorite save without corrupting the visible state',async()=>{
  const {saves,calls}=favoriteDispatch();
  render(<SkillPage snapshot={snapshot} refresh={vi.fn().mockResolvedValue(undefined)} notify={vi.fn()} onProject={vi.fn()}/>);
  fireEvent.click(await screen.findByRole('button',{name:'收藏 Writer'}));
  await act(async()=>saves[0].reject(new Error('磁盘不可写')));
  expect(screen.getByRole('alert')).toHaveTextContent('磁盘不可写');
  expect(screen.getByRole('button',{name:'收藏 Writer'})).toBeEnabled();
  fireEvent.click(screen.getByRole('button',{name:'收藏 Writer'}));
  expect(calls).toEqual(['s','s']);
  expect(saves[1]).toMatchObject({ids:['s'],favorite:true});
  await act(async()=>saves[1].resolve({skills:[{...snapshot.skills[0],favorite:true}]}));
  expect(screen.getByRole('button',{name:'取消收藏 Writer'})).toBeEnabled();
});
it('still saves a favorite while another Skill check is in flight',async()=>{
  let finishCheck!:(v:unknown)=>void;
  const check=new Promise(resolve=>{finishCheck=resolve;});
  const saved:Array<{ids:string[];favorite:boolean}>=[];
  vi.spyOn(api,'dispatch').mockImplementation((...[method,args])=>{
    if(method==='targets.list')return Promise.resolve({targets} as never);
    if(method==='skills.check')return check as never;
    if(method==='skills.metadata.save'){saved.push({ids:args.ids,favorite:!!args.favorite});return Promise.resolve({skills:[]} as never);}
    return Promise.resolve({} as never);
  });
  render(<SkillPage snapshot={snapshot} refresh={vi.fn().mockResolvedValue(undefined)} notify={vi.fn()} onProject={vi.fn()}/>);
  await screen.findByRole('button',{name:'收藏 Writer'});
  await userEvent.click(screen.getByRole('button',{name:'检查更新'}));
  expect(screen.getByRole('button',{name:'检查中…'})).toBeDisabled();
  await userEvent.click(screen.getByRole('button',{name:'收藏 Writer'}));
  expect(saved).toEqual([{ids:['s'],favorite:true}]);
  await act(async()=>finishCheck({skillId:'s',message:'已是最新'}));
});

it('does not install until the explicit reviewed plan is confirmed',async()=>{
  const dispatch=vi.spyOn(api,'dispatch').mockImplementation(async (...[method]) =>{if(method==='targets.list')return {targets} as never;if(method==='maintenance.get')return {} as never;if(method==='skills.install.preview')return plan as never;return {status:'succeeded',summary:'已安装'} as never;});
  const refresh=vi.fn().mockResolvedValue(undefined);render(<SkillPage snapshot={snapshot} refresh={refresh} notify={vi.fn()} onProject={vi.fn()}/>);
  await userEvent.click(await screen.findByRole('button',{name:'安装 Writer 到 Codex'}));
  expect(await screen.findByRole('dialog')).toHaveTextContent('C:\\agent\\skills\\writer');expect(dispatch).toHaveBeenCalledWith('skills.install.preview',{skillId:'s',targetId:'codex'});expect(dispatch.mock.calls.some(([m])=>m==='skills.install')).toBe(false);
  await userEvent.click(screen.getByRole('button',{name:'确认安装到 Agent'}));await waitFor(()=>expect(dispatch).toHaveBeenCalledWith('skills.install',{planId:'plan',confirmed:true}));await waitFor(()=>expect(refresh).toHaveBeenCalledTimes(1));
});
it('checks a source, shows file differences and updates only the selected locations',async()=>{
  const locations=[{id:'library',path:'C:/library/writer',label:'Skill 库',agents:[],exists:true,differences:[{path:'SKILL.md',change:'modified'}]},{id:'agent',path:'C:/agent/skills/writer',label:'Codex',agents:['codex'],exists:true,differences:[{path:'SKILL.md',change:'modified'}]}];
  const dispatch=vi.spyOn(api,'dispatch').mockImplementation(async (...[method]) =>{if(method==='targets.list')return {targets} as never;if(method==='maintenance.get')return {retainUpdateBackup:true} as never;if(method==='skills.check')return {skillId:'s',name:'Writer',checkId:'check',status:'available',message:'来源有变化',locations} as never;if(method==='skills.diff')return {oldText:'before',newText:'after'} as never;if(method==='skills.update.preview')return {...plan,action:'update',locations:[locations[1]]} as never;return {status:'succeeded',summary:'已更新'} as never;});
  render(<SkillPage snapshot={snapshot} refresh={vi.fn().mockResolvedValue(undefined)} notify={vi.fn()} onProject={vi.fn()}/>);
  await userEvent.click(screen.getByRole('button',{name:'检查更新'}));await userEvent.click(await screen.findByRole('button',{name:'更新'}));
  const dialog=within(screen.getByRole('dialog'));await userEvent.click(dialog.getByRole('button',{name:'SKILL.md'}));expect(await screen.findByText('before')).toBeVisible();expect(screen.getByText('after')).toBeVisible();
  await userEvent.click(dialog.getByLabelText('更新 Skill 库'));expect(dispatch.mock.calls.filter(([m])=>m==='skills.check')).toHaveLength(1);
  await userEvent.click(dialog.getByRole('button',{name:'确认更新'}));
  expect(dispatch).toHaveBeenCalledWith('skills.update.preview',{skillId:'s',checkId:'check',locationIds:['agent'],retainBackup:true});
  await waitFor(()=>expect(dispatch).toHaveBeenCalledWith('skills.update',{planId:'plan',confirmed:true}));
  expect(dispatch.mock.calls.filter(([m])=>m==='skills.update')).toHaveLength(1);
});
it('shows source setup for an unknown source and never offers a check',async()=>{
  vi.spyOn(api,'dispatch').mockImplementation(async (...[method]) =>method==='targets.list'?{targets:[]} as never:{} as never);
  render(<SkillPage snapshot={{...snapshot,skills:[{...snapshot.skills[0],source:{kind:'unknown',locator:''}}]}} refresh={vi.fn()} notify={vi.fn()} onProject={vi.fn()}/>);
  expect(await screen.findByRole('button',{name:'设置来源'})).toBeVisible();expect(screen.queryByRole('button',{name:'检查更新'})).not.toBeInTheDocument();
});

it('deletes directly after validating the selected scope, without a separate path step',async()=>{
  const dispatch=vi.spyOn(api,'dispatch').mockImplementation(async (...[method]) =>{
    if(method==='targets.list')return {targets} as never;
    if(method==='skills.delete.preview')return {...plan,action:'delete'} as never;
    if(method==='skills.delete')return {status:'succeeded',summary:'已删除'} as never;
    return {} as never;
  });
  const refresh=vi.fn().mockResolvedValue(undefined);
  render(<SkillPage snapshot={snapshot} refresh={refresh} notify={vi.fn()} onProject={vi.fn()}/>);
  await userEvent.click(screen.getByRole('button',{name:'从库删除'}));
  expect(dispatch.mock.calls.some(([m])=>m==='skills.delete')).toBe(false);
  await userEvent.click(screen.getByRole('button',{name:'直接删除'}));
  await waitFor(()=>expect(refresh).toHaveBeenCalledOnce());
  expect(dispatch).toHaveBeenCalledWith('skills.delete.preview',{skillId:'s',removeDeployments:false});
  expect(dispatch).toHaveBeenCalledWith('skills.delete',{planId:'plan',confirmed:true});
});

it('does not execute direct deletion when the core blocks the plan',async()=>{
  const dispatch=vi.spyOn(api,'dispatch').mockImplementation(async (...[method]) =>method==='targets.list'?{targets} as never:method==='skills.delete.preview'?{...plan,canExecute:false,blockedReason:'文件已变化'} as never:{} as never);
  render(<SkillPage snapshot={snapshot} refresh={vi.fn()} notify={vi.fn()} onProject={vi.fn()}/>);
  await userEvent.click(screen.getByRole('button',{name:'从库删除'}));
  await userEvent.click(screen.getByRole('button',{name:'直接删除'}));
  expect(await within(screen.getByRole('dialog')).findByRole('alert')).toHaveTextContent('文件已变化');
  expect(dispatch.mock.calls.some(([m])=>m==='skills.delete')).toBe(false);
});

it('checks all bound Skills and continues after a failure, skipping unknown sources',async()=>{
  const dispatch=vi.spyOn(api,'dispatch').mockImplementation(async (...[method,args]) =>{
    if(method==='targets.list')return {targets} as never;
    if(method==='skills.checkBatch.start')return {batchId:'batch',concurrency:3} as never;
    if(method==='skills.check'){if(args?.skillId==='s')throw Error('网络失败');return {skillId:'s2',message:'已是最新'} as never;}
    return {} as never;
  });
  render(<SkillPage snapshot={{...snapshot,skills:[snapshot.skills[0],{...snapshot.skills[0],id:'s2',name:'Second'},{...snapshot.skills[0],id:'s3',name:'Unknown',source:{kind:'unknown',locator:''}}]}} refresh={vi.fn()} notify={vi.fn()} onProject={vi.fn()}/>);
  await userEvent.click(screen.getByRole('button',{name:'检查全部更新'}));
  expect(await screen.findByText('已是最新')).toBeVisible();
  expect(dispatch.mock.calls.flatMap(([m,args])=>m==='skills.check'?[args.skillId]:[])).toEqual(['s','s2']);
  expect(screen.getByRole('alert')).toHaveTextContent('1 个 Skill 检查失败（Writer）');
});

it('offers no read location even when an installed copy exists',async()=>{
  const dispatch=vi.spyOn(api,'dispatch').mockImplementation(async (...[method]) =>method==='targets.list'?{targets} as never:method==='skills.files'?{files:[{path:'SKILL.md',size:1}]} as never:{content:'Library body'} as never);
  const deployment={id:'deployed',skillId:'s',name:'Writer',description:'',agent:'codex',scope:'global' as const,path:'C:/agent/skills/writer',source,owner:'user' as const,status:'present',ignored:false,present:true};
  render(<SkillPage snapshot={{...snapshot,deployments:[deployment]}} refresh={vi.fn()} notify={vi.fn()} onProject={vi.fn()}/>);
  await userEvent.click(await screen.findByRole('button',{name:'Writer'}));
  expect(await screen.findByText('Library body')).toBeVisible();
  const dialog=within(screen.getByRole('dialog'));
  expect(dialog.queryByLabelText('阅读位置')).not.toBeInTheDocument();
  expect(dialog.queryByRole('combobox')).not.toBeInTheDocument();
  expect(dialog.queryByText('C:\\agent\\skills\\writer')).not.toBeInTheDocument();
  expect(dispatch).toHaveBeenCalledWith('skills.files',{skillId:'s'});
  expect(dispatch.mock.calls.flatMap(([m,args])=>m==='skills.files'||m==='skills.read'?[args]:[]).every(args=>!('deploymentId' in args))).toBe(true);
});
it('filters all Agents to the selected project and can install another library Skill there',async()=>{
 const project={id:'p',name:'Project',path:'C:/project',gitTrusted:false,archived:false,createdAt:'',updatedAt:''};
 const local={...snapshot.skills[0],id:'local',name:'Project writer'};
 const dispatch=vi.spyOn(api,'dispatch').mockImplementation(async (...[method]) =>method==='targets.list'?{targets} as never:method==='skills.install.preview'?plan as never:{} as never);
 render(<SkillPage snapshot={{...snapshot,skills:[snapshot.skills[0],local],projects:[project],deployments:[{id:'global',skillId:'s',name:'Writer',description:'',agent:'codex',scope:'global',path:'C:/agent/skills/writer',source,owner:'user',status:'present',ignored:false,present:true},{id:'project',skillId:'local',name:'Project writer',description:'',agent:'codex',scope:'project',projectId:'p',path:'C:/project/.agents/skills/writer',source,owner:'user',status:'present',ignored:false,present:true}]}} projectId="p" refresh={vi.fn()} notify={vi.fn()} onProject={vi.fn()}/>);
 expect(await screen.findByRole('button',{name:'Project writer'})).toBeVisible();
 expect(screen.queryByRole('button',{name:'Writer'})).not.toBeInTheDocument();
 await userEvent.click(screen.getByRole('button',{name:'从库安装'}));
 await userEvent.click(await within(screen.getByRole('dialog')).findByRole('button',{name:'安装 Writer 到 Codex'}));
 expect(dispatch).toHaveBeenCalledWith('skills.install.preview',{skillId:'s',targetId:'codex',projectId:'p'});
 expect(dispatch.mock.calls.some(([m])=>m==='skills.install')).toBe(false);
});
