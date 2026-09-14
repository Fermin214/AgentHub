import type { Snapshot } from './types';
import type { DispatchCall, Method, Response } from './contracts';
const date = '2026-09-12T00:00:00Z';
export const previewSnapshot: Snapshot = {
  dataScope: 'static-preview', dataDir: '桌面应用的本地数据目录',
  prompts: [{id:'prompt-example',title:'把问题说明白',body:'先说明目标和已知事实，再列出仍需确认的问题。',category:'',tags:['思考'],favorite:true,createdAt:date,updatedAt:date}],
  skills:[{id:'skill-example',name:'写作助手',description:'示例 Skill，帮助整理文本。',path:'Skill 库 / 写作助手',source:{kind:'unknown',locator:''},tags:['写作'],favorite:true,createdAt:date,updatedAt:date}],
  deployments:[],projects:[],settings:{scanRoots:[],executables:{codex:'',claude:'',dsh:''}},operations:[],
};
export async function dispatchDemo<M extends Method>(...[method]: DispatchCall<M>): Promise<Response<M>> {
  const reads: Partial<{ [K in Method]: Response<K> }> = {
    snapshot: previewSnapshot,
    'targets.list': {targets:[]}, 'repositories.list': {repositories:[]}, 'bookmarks.list': {items:[]}, 'bookmarks.sync':{items:[]} ,
    'backups.list':{backups:[]}, 'network.get':{proxyUrl:''},
    'maintenance.get':{automaticChecks:false,intervalHours:24,retainUpdateBackup:true,maxBackups:null},
    'maintenance.preview':{maxBackups:null,pruneCount:0,protectedCount:0},
  };
  if (!Object.hasOwn(reads, method)) throw new Error('请在桌面应用中执行此操作。当前页面为只读预览。');
  return structuredClone(reads[method]) as Response<M>;
}
