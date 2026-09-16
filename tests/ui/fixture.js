// Deterministic frontend transport only. No filesystem, real Agent, or network writes.
import React from 'react';
import {createRoot} from 'react-dom/client';
import App from '/src/App.tsx';
const date='2025-01-01T00:00:00Z';
const source={kind:'git',locator:'https://example.invalid/fixture/skills'};
const names={codex:'Codex',claude:'Claude Code',dsh:'DeepSeek Harness',zcode:'ZCode',hermes:'Hermes'};
const targets=Object.entries(names).map(([id,name])=>({id,name,enabled:true,available:true,globalPath:`C:/AcceptanceFixture/agents/${id}/skills`,projectPath:'.agents/skills'}));
const skills=[{id:'long',name:'写作助手 · 长文示例',description:'点击名称检查左右独立滚动、键盘翻页和来源仓库入口。',source,path:'C:/AcceptanceFixture/library/long',tags:['写作','示例'],favorite:true,createdAt:date,updatedAt:date},{id:'local',name:'本机 Skill',description:'本机来源保留文件查看器，不显示来源仓库外链。',source:{kind:'local',locator:'C:/AcceptanceFixture/source'},path:'C:/AcceptanceFixture/library/local',tags:[],favorite:false,createdAt:date,updatedAt:date}];
const snapshot={dataScope:'isolated-preview-011',dataDir:'仅用于预览的示例数据',prompts:[{id:'p1',title:'把问题说明白',body:'先说明目标和已知事实，再列出仍需确认的问题。',category:'思考',tags:['分析','表达','常用'],favorite:true,createdAt:date,updatedAt:date},{id:'p2',title:'修改一段文字',body:'保留原意，调整表达顺序，让读者第一次阅读就能理解。',category:'写作',tags:['编辑','长标签用于检查换行与留白'],favorite:false,createdAt:date,updatedAt:date}],skills,deployments:[{id:'d1',skillId:'long',name:skills[0].name,description:'示例安装',agent:'codex',scope:'global',path:targets[0].globalPath+'/long',source,owner:'atb',status:'installed',present:true,ignored:false}],projects:[],settings:{scanRoots:[],executables:{codex:'',claude:'',dsh:'',zcode:'',hermes:''},language:'zh'},operations:[]};
let request;
// Parked favorite saves and their per-request release hooks. Preview data only.
window.pendingFavorite=[];
window.releaseFavorite=()=>{const next=window.pendingFavorite.shift();if(next)next({skills:clone(snapshot.skills)});};
const pause=ms=>new Promise(resolve=>setTimeout(resolve,ms));
const clone=value=>structuredClone(value);
window.__TAURI_INTERNALS__={invoke:async(command,{method,args={}}={})=>{
  if(command!=='dispatch')throw new Error('这个操作请在正式桌面版中使用。');
  if(method==='snapshot')return clone(snapshot);
  if(method==='targets.list')return {targets};
  if(method==='repositories.list')return {repositories:[]};
  if(method==='backups.list')return {backups:[]};
  if(method==='maintenance.get')return {automaticChecks:false,intervalHours:24,retainUpdateBackup:true,maxBackups:null};
  if(method==='network.get')return {proxyUrl:''};
  if(method==='settings.save'){snapshot.settings=args.settings;return clone(snapshot);}
  // `prompts.save` returns the saved Prompt, matching the production contract.
  if(method==='prompts.save'){const prompt=args.prompt.id?args.prompt:{...args.prompt,id:'prompt-preview'};snapshot.prompts=snapshot.prompts.some(p=>p.id===prompt.id)?snapshot.prompts.map(p=>p.id===prompt.id?prompt:p):[prompt,...snapshot.prompts];return clone(prompt);}
  if(method==='bookmarks.list'||method==='bookmarks.sync')return {items:[{id:'b1',name:'AgentHub',url:source.locator,notes:'可选中这段备注；点击卡片空白处不会打开网页。',status:'interested',archived:false}]};
  if(method==='skills.files')return {files:Array.from({length:180},(_,i)=>({path:i?'references/example-'+i+'.md':'SKILL.md',size:18000,previewable:true}))};
  if(method==='skills.read')return {content:'# '+(args.path||'SKILL.md')+'\n\n'+Array.from({length:500},(_,i)=>`${i+1}. 这是独立预览中的示例正文。聚焦正文区域后，可以使用 Page Down / Page Up。`).join('\n')};
  // Apply the patch by field so a tags-only save never rewrites `favorite`, and a
  // favorite-only save never rewrites `tags`, exactly like the production contract.
  const applied=()=>{snapshot.skills=snapshot.skills.map(skill=>{if(!args.ids.includes(skill.id))return skill;const next={...skill};if(args.favorite!==undefined)next.favorite=!!args.favorite;if(args.tags!==undefined)next.tags=args.tags;return next;});return {skills:clone(snapshot.skills)};};
  if(method==='skills.metadata.save'){
    // `pending` parks the request. Every parked save gets its own release hook, so
    // a test can complete concurrent saves one at a time in any order.
    if(window.favoriteRequest==='pending')return new Promise(resolve=>{window.pendingFavorite.push(()=>resolve(applied()));});
    if(window.favoriteRequest==='reject'){window.favoriteRequest='ok';throw {code:'UNKNOWN',detail:'磁盘不可写'};}
    return applied();
  }
  if(method==='sources.begin'){if(request&&!request.done)throw {code:'SOURCE_BUSY',detail:'A source inspection is still cleaning up.'};request={id:crypto.randomUUID(),started:Date.now(),cancelled:false,done:false,scenario:window.fixtureScenario || 'slow'};return {requestId:request.id};}
  if(method==='sources.cancel'){if(request?.id===args.requestId)request.cancelled=true;return {};}
  if(method==='sources.status'){const elapsedSeconds=Math.floor((Date.now()-request.started)/1000);return {stage:request.cancelled?'cancelling':elapsedSeconds<4?'cloning':elapsedSeconds<8?'scanning':'fingerprinting',elapsedSeconds,attempt:1};}
  if(method==='sources.inspect'){
    const job=request;
    while(Date.now()-job.started<(job.scenario==='slow'?90000:job.scenario==='error'?100:100)){
      await pause(80);
      if(job.cancelled){await pause(500);job.done=true;throw {code:'SOURCE_CANCELLED',detail:'Source inspection cancelled'};}
    }
    job.done=true;
    if(job.scenario==='error')throw {code:'UNKNOWN',detail:'Git clone failed: Failed to connect to example.test port 443 after 4000 ms'};
    return {inspectionId:'preview-snapshot',source:args.source,candidates:[{name:'示例 Skill',description:'这是模拟检查结果，用于检查选择和保存状态。',subpath:'skills/example'}]};
  }
  if(method==='sources.release')return {ok:true};
  if(method==='skills.add'){await pause(3000);return {skill:skills[0]};}
  if(method==='links.open'){window.open(args.url,'_blank','noopener,noreferrer');return {ok:true};}
  throw new Error('这个操作不在本次预览场景中。');
}};

createRoot(document.getElementById('root')).render(React.createElement(App));
