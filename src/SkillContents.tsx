import { useEffect, useState } from 'react';
import * as api from './api';
import { useT } from './i18n';
import type { Skill } from './types';
// The viewer always reads the Skill library copy. Agent installs are managed
// through the install/remove review flow, so no read location is selected here.
export function SkillContents({skill}:{skill:Skill}) {
  const t=useT();
  const [files,setFiles]=useState<Array<{path:string;size:number;previewable?:boolean}>>([]);const [path,setPath]=useState('');const [content,setContent]=useState('');const [error,setError]=useState('');const [busy,setBusy]=useState(false);
  useEffect(()=>{let live=true;setFiles([]);setPath('');setContent('');setError('');setBusy(true);api.dispatch('skills.files',{skillId:skill.id}).then(r=>{if(live){setFiles(r.files);setPath(r.files.find(f=>f.path==='SKILL.md')?.path||r.files.find(f=>f.previewable!==false)?.path||'');}}).catch(e=>{if(live)setError(String(e));}).finally(()=>{if(live)setBusy(false);});return()=>{live=false;};},[skill.id]);
  useEffect(()=>{if(!path)return;let live=true;setContent('');setError('');setBusy(true);api.dispatch('skills.read',{skillId:skill.id,path}).then(r=>{if(live)setContent(r.content);}).catch(e=>{if(live)setError(String(e));}).finally(()=>{if(live)setBusy(false);});return()=>{live=false;};},[skill.id,path]);
  return <section className="toolkit-reader"><h3>{t('contents.title')}</h3>{error&&<p role="alert" className="text-error">{t.backend(error)}</p>}<div className="toolkit-reader__layout"><nav tabIndex={0} aria-label={t('contents.files')}>{files.map(f=><button key={f.path} disabled={f.previewable===false} className={f.path===path?'active':''} onClick={()=>setPath(f.path)}>{f.path}</button>)}</nav><div className="toolkit-reader__content" role="region" aria-label={t('contents.text')} tabIndex={0}>{busy?<p role="status">{t('contents.reading')}</p>:<><h4>{path}</h4><pre><code>{content}</code></pre></>}</div></div></section>;
}
