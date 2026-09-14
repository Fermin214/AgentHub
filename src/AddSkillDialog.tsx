import { useEffect, useRef, useState } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import * as api from './api';
import { parseSkillSourceInput, selectCommandSkills } from './skillSourceInput';
import { ExistingSkillsImport } from './ExistingSkillsImport';
import { SkillCandidateDetails } from './SkillCandidateDetails';
import { Modal, Button, TabList } from './ui';
import { useT } from './i18n';
import type { Skill, SkillCandidate, SkillDeployment, Source } from './types';
type Inspection = { source: Source; candidates: SkillCandidate[]; inspectionId: string };
type Props = { deployments: SkillDeployment[]; refresh: () => Promise<void>; onClose: () => void; bindSkill?: Skill };
export function AddSkillDialog({deployments,refresh,onClose,bindSkill}: Props) {
  const t=useT();
  const [tab,setTab]=useState('source');
  const [locator,setLocator]=useState(bindSkill?.source.locator||'');
  const [revision,setRevision]=useState(bindSkill?.source.revision||'');
  const [inspection,setInspection]=useState<Inspection>();
  const [selected,setSelected]=useState<string[]>([]);
  const [completed,setCompleted]=useState<string[]>([]);
  const [repositories,setRepositories]=useState<Array<{id:string;source:Source;derived?:boolean}>>([]);
  const [repositoryId,setRepositoryId]=useState('');
  const [busy,setBusy]=useState(false);
  const [error,setError]=useState('');
  const currentInspection=useRef<string>();
  const mounted=useRef(true);
  const release=()=>{const id=currentInspection.current;currentInspection.current=undefined;if(id)void api.dispatch('sources.release',{inspectionId:id}).catch(()=>{});};
  useEffect(()=>{mounted.current=true;api.dispatch('repositories.list').then(r=>{if(mounted.current)setRepositories(r.repositories);}).catch(e=>{if(mounted.current)setError(String(e));});return()=>{mounted.current=false;release();};},[]);
  const reset=()=>{release();setInspection(undefined);setSelected([]);setCompleted([]);setError('');};
  const inspect=async()=>{setBusy(true);reset();try{
    const saved=repositories.find(r=>r.id===repositoryId)?.source;
    const local=/^[A-Za-z]:|^\\\\|^\//.test(locator.trim());
    const parsed=saved?{source:saved,skillNames:[]}:parseSkillSourceInput(locator,local?'local':/\.zip(?:[?#]|$)/i.test(locator)?'zip':'git',revision||undefined,t);
    const result=await api.dispatch('sources.inspect',{source:parsed.source});
    if(!mounted.current){await api.dispatch('sources.release',{inspectionId:result.inspectionId});return;}
    currentInspection.current=result.inspectionId;result.candidates=selectCommandSkills(result.candidates,parsed.skillNames,t);
    if(bindSkill){
      const matching=result.candidates.filter(c=>c.name.trim().toLowerCase()===bindSkill.name.trim().toLowerCase());
      result.candidates=matching;
      if(matching.length===1)setSelected([matching[0].subpath]);
      if(!matching.length)setError(t('add.noMatch',{name:bindSkill.name}));
    }
    setInspection(result);
    if(!result.candidates.length&&!bindSkill)setError(t('add.noSkillMd'));
  }catch(e){if(mounted.current)setError(String(e));}finally{if(mounted.current)setBusy(false);}};
  // A candidate already saved in the library is reported by the inspection itself.
  const settled=(c:SkillCandidate)=>completed.includes(c.subpath)||(!bindSkill&&!!c.skillId);
  const pending=(list:SkillCandidate[])=>list.filter(c=>!settled(c));
  const add=async()=>{if(!inspection)return;setBusy(true);setError('');const failures:string[]=[];try{
    for(const subpath of selected.filter(s=>!completed.includes(s))){try{await (bindSkill?api.dispatch('skills.bindSource',{inspectionId:inspection.inspectionId,subpath,skillId:bindSkill.id}):api.dispatch('skills.add',{inspectionId:inspection.inspectionId,subpath}));setCompleted(v=>[...v,subpath]);}catch(e){failures.push(t('check.skillFailed',{name:subpath||t('add.currentSkill'),message:t.backend(String(e))}));}}
    await refresh();if(failures.length)setError(failures.join('\n'));else onClose();
  }catch(e){setError(String(e));}finally{setBusy(false);}};
  return <Modal title={bindSkill?t('add.bindTitle',{name:bindSkill.name}):t('add.title')} wide onClose={busy?()=>{}:onClose} footer={tab==='source'?<><span className="modal-footer-note">{inspection?bindSkill?t('add.selectedOne',{name:inspection.candidates.find(c=>selected.includes(c.subpath))?.name||'—'}):t('add.foundCount',{n:inspection.candidates.length}):bindSkill?t('add.bindHint'):''}</span><Button variant="primary" loading={busy} disabled={!selected.some(s=>!completed.includes(s))} onClick={()=>void add()}>{bindSkill?t('add.saveSource'):t('add.addCount',{n:selected.filter(s=>!completed.includes(s)).length})}</Button></>:undefined}>
    {!bindSkill&&<TabList id="add-skill" label={t('add.title')} className="add-skill__tabs" value={tab} disabled={busy} items={[{id:'source',label:t('add.fromSource')},{id:'existing',label:t('add.fromLocal')}]} onChange={setTab}/>}
    <div role={bindSkill?undefined:'tabpanel'} id="add-skill-panel" aria-labelledby={bindSkill?undefined:`add-skill-tab-${tab}`}>
    {tab==='existing'?<ExistingSkillsImport deployments={deployments} refresh={refresh} onClose={onClose} onBusyChange={setBusy}/>:<>
      <div className="source-form">{repositories.length>0&&<label className="field"><span>{t('add.savedSources')}</span><select disabled={busy} value={repositoryId} onChange={e=>{reset();setRepositoryId(e.target.value);const source=repositories.find(r=>r.id===e.target.value)?.source;setLocator(source?.locator||'');setRevision(source?.revision||'');}}><option value="">{t('add.newAddress')}</option>{repositories.map(r=><option key={r.id} value={r.id}>{r.source.locator}</option>)}</select></label>}
      <label className="field"><span>{t('add.source')}</span><div className="add-skill__source"><input aria-label={t('add.source')} value={locator} disabled={busy} placeholder={t('add.sourcePlaceholder')} onChange={e=>{reset();setRepositoryId('');setLocator(e.target.value);}}/><Button disabled={busy} onClick={()=>{if(api.isTauriRuntime())void open({directory:true,multiple:false}).then(p=>{if(typeof p==='string'){reset();setRepositoryId('');setLocator(p);}}).catch(e=>setError(String(e)));else setError(t('common.desktopOnlyFolder'));}}>{t('common.chooseFolder')}</Button><Button disabled={busy||!locator.trim()} onClick={()=>void inspect()}>{bindSkill?t('add.findSource'):t('add.findSkills')}</Button></div></label>
      <label className="field source-revision"><span>{t('add.revision')}</span><input placeholder={t('add.defaultBranch')} value={revision} disabled={busy||!!repositoryId} onChange={e=>{reset();setRevision(e.target.value);}}/></label>
      </div>
      {inspection&&<div className="add-skill__candidates"><h3 className="list-heading">{bindSkill?t('add.chooseMatch'):t('add.found')}</h3>{!bindSkill&&<label className="check-row"><input type="checkbox" disabled={busy||!pending(inspection.candidates).length} checked={pending(inspection.candidates).length>0&&pending(inspection.candidates).every(c=>selected.includes(c.subpath))} onChange={e=>setSelected(e.target.checked?pending(inspection.candidates).map(c=>c.subpath):completed)}/>{t('add.selectAll')}</label>}{inspection.candidates.map(c=><label className="import-skills__item" key={c.subpath}><input type={bindSkill?'radio':'checkbox'} name="source-candidate" disabled={busy||settled(c)} checked={selected.includes(c.subpath)} onChange={e=>setSelected(v=>bindSkill?[c.subpath]:e.target.checked?[...v,c.subpath]:v.filter(s=>s!==c.subpath))}/><span><strong>{c.name}{completed.includes(c.subpath)?t('add.alreadyAdded'):settled(c)?t('add.alreadyInLibrary'):''}</strong><SkillCandidateDetails candidate={c}/><small className="candidate-description">{c.description}</small></span></label>)}</div>}
      {error&&<p role="alert" className="text-error">{t.backend(error)}</p>}
    </>}
    </div>
  </Modal>;
}
