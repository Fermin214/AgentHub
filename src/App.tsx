import { useCallback, useEffect, useRef, useState } from 'react';
import { Clipboard, Package, FolderOpen, Settings2, Menu, Info, LoaderCircle } from 'lucide-react';
import appIcon from './assets/app-icon.png';
import * as api from './api';
import { PromptsPage } from './PromptsPage';
import { SkillPage } from './SkillPage';
import { ProjectsPage } from './ProjectsPage';
import { SettingsPage } from './SettingsPage';
import { useSkillUpdates } from './useSkillUpdates';
import { useAutomaticUpdates } from './useAutomaticUpdates';
import { Toast, type Notify } from './ui';
import { LanguageProvider, resolveLang, useT, type Lang } from './i18n';
import type { BackupRecord, Skill, Snapshot, UpdateCheck } from './types';
import './styles.css';
import './skillLibrary.css';

const pages = [{ id: 'prompts', icon: Clipboard }, { id: 'skills', icon: Package }, { id: 'projects', icon: FolderOpen }, { id: 'settings', icon: Settings2 }] as const;

/// The stored language is only known once the snapshot loads, so the shell owns
/// the resolved value and re-provides it whenever a save changes the setting.
export default function App() {
  const [lang, setLang] = useState<Lang>(() => resolveLang());
  return <LanguageProvider lang={lang}><Workspace lang={lang} onLang={setLang} /></LanguageProvider>;
}

function Workspace({ lang, onLang }: { lang: Lang; onLang: (lang: Lang) => void }) {
  const t = useT();
  const [page, setPage] = useState<string>('prompts');
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [backups, setBackups] = useState<BackupRecord[]>([]);
  const [updates, setUpdates] = useState<UpdateCheck[]>([]);
  const [toast, setToast] = useState<{message: string; tone: 'success' | 'error' | 'info'}>();
  const [error, setError] = useState('');
  const [mobile, setMobile] = useState(false);
  const [projectId, setProjectId] = useState<string>();
  const checking = useRef(false);
  const refreshSequence=useRef(0);
  // Toasts carry both dictionary text and raw core errors; `t.backend` returns
  // anything it does not recognise unchanged, so one pass here covers both.
  const notify: Notify = useCallback((message, tone = 'success') => setToast({message:t.backend(message),tone}), [t]);
  const refresh = useCallback(async () => { const sequence=++refreshSequence.current; const result = await api.getSnapshot(); if(sequence===refreshSequence.current){setSnapshot(result);setError('');} }, []);
  // `skills.metadata.save` only returns the records it touched, so merge them by id
  // and keep every other Skill. Replacing the list here would drop unrelated rows
  // until the next snapshot, and permanently if that snapshot fails.
  const applySkills = useCallback((saved: Skill[]) => setSnapshot(current => current ? { ...current, skills: current.skills.map(skill => saved.find(item => item.id === skill.id) ?? skill) } : current), []);
  const skillUpdates=useSkillUpdates(snapshot,refresh,notify);
  useEffect(() => { void refresh().catch(e => setError(String(e))); }, [refresh]);
  // The saved language wins over the system locale once local data is readable.
  useEffect(() => { if (snapshot) onLang(resolveLang(snapshot.settings.language)); }, [snapshot?.settings.language, onLang]);
  useEffect(() => { document.documentElement.lang = lang === 'zh' ? 'zh-CN' : 'en'; }, [lang]);
  useEffect(() => { if (!toast) return; const timer = setTimeout(() => setToast(undefined), 5000); return () => clearTimeout(timer); }, [toast]);
  useEffect(() => { if (page !== 'settings') return; let live = true; const load = () => api.listBackups().then(r => { if(live) setBackups(r.backups); }).catch(e => notify(String(e),'error')); void load(); window.addEventListener('agenthub-backups-changed',load); return () => {live=false;window.removeEventListener('agenthub-backups-changed',load);}; }, [page, snapshot, notify]);
  useEffect(()=>{if(page==='settings')void refresh().catch(e=>notify(String(e),'error'));},[page,refresh,notify]);
  const check = async () => { if(checking.current) return; checking.current=true; try {const result=await api.checkUpdates();setUpdates(result.updates);await refresh();} finally {checking.current=false;} };
  useAutomaticUpdates(api.isTauriRuntime() && snapshot?.recovery?.status !== 'restricted' ? snapshot?.dataScope : undefined, skillUpdates.checkingIds.length>0||!!skillUpdates.checkProgress, check);
  if(!snapshot) return <div className="app-shell app-shell--loading">{error ? <div className="fatal-error"><h2>{t('app.loadFailed')}</h2><p role="alert">{t.backend(error)}</p><button className="button button--primary" onClick={()=>void refresh().catch(e=>setError(String(e)))}>{t('app.retry')}</button></div> : <p role="status"><LoaderCircle className="spin"/> {t('app.opening')}</p>}</div>;
  return <div className="app-shell">
    <aside className={'sidebar'+(mobile?' sidebar--open':'')}><div className="sidebar__top"><div className="brand"><span className="brand-mark"><img src={appIcon} alt=""/></span><span>{t('app.name')}</span></div></div><nav className="sidebar__nav" aria-label={t('nav.main')}>{pages.map(({id,icon:Icon})=><button key={id} className={'nav-item'+(page===id?' nav-item--active':'')} aria-current={page===id?'page':undefined} onClick={()=>{setPage(id);setMobile(false);}}><span className="nav-item__icon"><Icon size={18}/></span><span className="nav-item__copy"><strong>{t('nav.'+id)}</strong><small>{t('nav.'+id+'.hint')}</small></span></button>)}</nav></aside>
    {mobile&&<button className="sidebar-overlay" aria-label={t('nav.close')} onClick={()=>setMobile(false)}/>}
    <main className="main-content"><div className="mobile-topbar"><button className="icon-button" aria-label={t('nav.open')} onClick={()=>setMobile(true)}><Menu/></button>{t('app.name')}</div>{!api.isTauriRuntime()&&<div className="demo-banner"><Info size={16}/><span><strong>{t('demo.readonly')}</strong> {t('demo.body')}</span></div>}<div className="page-wrap">
      {snapshot.recovery?.status==='restricted'&&<section className="recovery-banner" role="alert"><h2>{t('recovery.title')}</h2><p>{t('recovery.body')}</p><details><summary>{t('recovery.details')}</summary><p>{snapshot.recovery.code}</p><pre>{snapshot.recovery.detail}</pre>{snapshot.recovery.issues.map((issue,index)=><div key={issue.id||index}><strong>{issue.id||issue.state}</strong><ul>{issue.paths.map(path=><li key={path}>{path}</li>)}</ul></div>)}</details><button className="button" onClick={()=>void refresh().catch(e=>notify(String(e),'error'))}>{t('recovery.retry')}</button></section>}
      {page==='prompts'&&<PromptsPage prompts={snapshot.prompts} onSnapshot={setSnapshot} notify={notify}/>}
      {page==='skills'&&<SkillPage snapshot={snapshot} refresh={refresh} notify={notify} controller={skillUpdates} projectId={projectId} onProject={setProjectId} onSkills={applySkills}/>}
      {page==='projects'&&<ProjectsPage projects={snapshot.projects} updates={updates} dataScope={snapshot.dataScope||''} refresh={refresh} onManageSkills={id=>{setProjectId(id);setPage('skills');}}/>}
      {page==='settings'&&<SettingsPage dataDir={snapshot.dataDir} settings={snapshot.settings} backups={backups} operations={snapshot.operations} notify={notify} onSave={async(settings,message)=>{try{await api.saveSettings(settings);await refresh();notify(message||t('settings.saved'));}catch(e){notify(String(e),'error');}}} onRestore={async id=>{try{const result=await api.restoreBackup(id);await refresh();notify(result.summary,result.status==='succeeded'?'success':'error');}catch(e){notify(String(e),'error');}}}/>}
    </div></main>{toast&&<Toast {...toast} onClose={()=>setToast(undefined)}/>}
  </div>;
}
