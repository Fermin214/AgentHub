import { useEffect, useState } from 'react';
import { ArchiveRestore, HardDrive, History, RotateCcw, Trash2, Plus, FolderOpen } from 'lucide-react';
import { version as appVersion } from '../package.json';
import { TargetManager, type AgentTarget } from './TargetManager';
import { BackupRetentionSettings, NetworkSettings, MaintenanceSettings, DependencyStatus } from './PreferencesPanel';
import * as api from './api';
import { displayPath } from './displayPath';
import { agentLabels, scopeLabels, type AgentKey, type BackupRecord, type DeploymentScope, type ScanRoot, type Settings, type Snapshot } from './types';
import { LANGUAGES, makeTranslate, resolveLang, useT, type Translate } from './i18n';
import { Button, IconButton, Badge, Modal, PageHeader, TabList, copyValue, makeId } from './ui';
export function SettingsPage({ dataDir, notify, settings, backups, operations, onSave, onRestore }: { dataDir?:string; notify: (message:string,tone?:'success'|'error'|'info')=>void; settings: Settings; backups: BackupRecord[]; operations: Snapshot['operations']; onSave: (settings: Settings, message?: string) => Promise<void>; onRestore: (id: string) => Promise<void> }) {
  const t=useT();
  const [group,setGroup]=useState('agent');
  const [targetRevision,setTargetRevision]=useState(0);
  const scopes=scopeLabels(t);
  const [knownTargets,setKnownTargets]=useState<AgentTarget[]>([]);
  useEffect(()=>{let live=true;api.dispatch('targets.list').then(r=>{if(live)setKnownTargets(r.targets||[]);}).catch(e=>notify(String(e),'error'));return()=>{live=false;};},[targetRevision]);
  const agentName=(id:string)=>knownTargets.find(t=>t.id===id)?.name||agentLabels(t)[id as AgentKey]||id;
  const [draft, setDraft] = useState<Settings>(() => copyValue(settings));
  const [rootEditor, setRootEditor] = useState(false);
  const [restoreBackup, setRestoreBackup] = useState<BackupRecord | null>(null);
  useEffect(() => setDraft(copyValue(settings)), [settings]);
  const executableKey = (id: string) => id as keyof Settings['executables'];
  const setExecutable = (key: keyof Settings['executables'], value: string) => setDraft((current) => ({ ...current, executables: { ...current.executables, [key]: value } }));
  // A program path is only needed where automatic detection failed, and stays
  // editable afterwards so a wrong path can be corrected or cleared.
  const undetected = knownTargets.filter((target) => target.enabled && (target.available === false || !!settings.executables[executableKey(target.id)]));
  const addRoot = (root: ScanRoot) => { setDraft((current) => ({ ...current, scanRoots: current.scanRoots.some(r=>r.agent===root.agent&&r.scope===root.scope&&r.profile===root.profile&&displayPath(r.path).toLowerCase()===displayPath(root.path).toLowerCase())?current.scanRoots:[...current.scanRoots, root] })); setRootEditor(false); };
  const removeRoot = (id: string) => setDraft((current) => ({ ...current, scanRoots: current.scanRoots.filter((root) => root.id !== id) }));
  return <>
    <PageHeader title={t('settings.title')} description={t('settings.description')} />
    <TabList id="settings" label={t('settings.groups')} value={group} items={['agent','updates','data','about'].map(id=>({id,label:t('settings.tab.'+id)}))} onChange={id=>{setGroup(id);setDraft(copyValue(settings));}}/>
    <div className="settings-groups" role="tabpanel" id="settings-panel" aria-labelledby={`settings-tab-${group}`}>
      {group==='agent'&&<><TargetManager notify={notify} onChanged={()=>setTargetRevision(v=>v+1)}/><section className="settings-card"><h2>{t('settings.extraRoots')}</h2><p>{t('settings.extraRoots.body')}</p><Button variant="secondary" size="sm" onClick={()=>setRootEditor(true)} icon={<Plus size={14}/>}>{t('settings.extraRoots.add')}</Button><div className="scan-root-list">{draft.scanRoots.filter(root=>!knownTargets.some(known=>root.scope==='global'&&displayPath(root.path).toLowerCase()===displayPath(known.globalPath).toLowerCase())).map(root=><div className="scan-root-row" key={root.id}><div><strong>{agentName(root.agent)}{root.profile?' / '+root.profile:''}</strong><p className="library-path">{displayPath(root.path)}</p></div><Badge tone="neutral">{scopes[root.scope]}</Badge><IconButton label={t('settings.extraRoots.removeLabel',{path:displayPath(root.path)})} onClick={()=>removeRoot(root.id)}><Trash2 size={15}/></IconButton></div>)}</div><Button variant="primary" disabled={JSON.stringify(draft.scanRoots)===JSON.stringify(settings.scanRoots)} onClick={()=>void onSave({...settings,scanRoots:draft.scanRoots})}>{t('settings.extraRoots.save')}</Button></section>{undetected.length>0&&<section className="settings-card"><h2>{t('settings.executables')}</h2><p>{t('settings.executables.body')}</p><div className="executable-fields">{undetected.map(target=><label className="field" key={target.id}><span>{t('settings.executables.field',{name:target.name})}</span><div className="toolkit-path-picker"><input value={draft.executables[executableKey(target.id)]||''} onChange={e=>setExecutable(executableKey(target.id),e.target.value)} placeholder={t('settings.executables.auto')}/><button className="button button--secondary" onClick={()=>void api.pickExecutable(t('dialog.fileFilter')).then(p=>{if(typeof p==='string')setExecutable(executableKey(target.id),p);}).catch(e=>notify(String(e),'error'))}>{t('common.browse')}</button></div></label>)}</div><Button variant="primary" disabled={JSON.stringify(draft.executables)===JSON.stringify(settings.executables)} onClick={()=>void onSave({...settings,executables:draft.executables})}>{t('settings.executables.save')}</Button></section>}</>}
      {group==='updates'&&<><MaintenanceSettings notify={notify}/><NetworkSettings notify={notify}/><DependencyStatus/></>}
      {group==='data'&&<><BackupRetentionSettings notify={notify}/><section className="side-card"><div className="side-card__heading"><ArchiveRestore size={17} /><h2>{t('settings.backups')}</h2></div><p>{t('settings.backups.body')}</p>{backups.length ? <div className="backup-list">{backups.map((backup) => <div className="backup-row" key={backup.id}><HardDrive size={15} /><div><strong>{t.backend(backup.name)}</strong><span>{t.relative(backup.createdAt)}</span></div><IconButton label={t('settings.backups.restoreLabel',{name:t.backend(backup.name)})} onClick={() => setRestoreBackup(backup)}><RotateCcw size={15} /></IconButton></div>)}</div> : <div className="side-empty"><ArchiveRestore size={19} /><span>{t('settings.backups.empty')}</span><small>{t('settings.backups.emptyHint')}</small></div>}</section><section className="side-card"><div className="side-card__heading"><History size={17} /><h2>{t('settings.operations')}</h2></div>{operations.length ? <div className="operation-list">{operations.slice(0, 5).map((operation) => <details className="operation-detail" key={operation.id}><summary className="operation-log"><span className={`operation-log__dot operation-log__dot--${operation.status}`} /><div><strong>{t.backend(operation.summary)}</strong><span>{t('settings.operations.detail',{time:t.relative(operation.createdAt)})}</span></div></summary><div className="operation-detail__body"><p>{t.backend(operation.summary)}</p><p className="operation-detail__meta"><span>{t.dateTime(operation.createdAt)}</span><span>{t('settings.operations.'+operation.status)}</span></p>{operation.locations?.map(path=><p className="library-path" key={path}>{displayPath(path)}</p>)}{operation.warnings?.map(message=><p key={message}>{t.backend(message)}</p>)}</div></details>)}</div> : <div className="side-empty"><History size={19} /><span>{t('settings.operations.empty')}</span><small>{t('settings.operations.emptyHint')}</small></div>}</section></>}
      {group==='about'&&<><LanguageSettings settings={settings} onSave={onSave}/><section className="settings-card"><h2>{t('app.name')} {appVersion}</h2><p>{t('settings.about.body')}</p>{dataDir&&<p className="library-path">{t('settings.about.dataDir',{path:displayPath(t.backend(dataDir))})}</p>}</section></>}
    </div>
    {rootEditor && <ScanRootDialog targets={knownTargets} onClose={() => setRootEditor(false)} onSave={addRoot} />}
    {restoreBackup && <Modal title={t('settings.restore.title')} onClose={() => setRestoreBackup(null)} footer={<><Button variant="secondary" onClick={() => setRestoreBackup(null)}>{t('common.cancel')}</Button><Button variant="primary" onClick={() => { void onRestore(restoreBackup.id); setRestoreBackup(null); }} icon={<RotateCcw size={15} />}>{t('settings.restore.confirm')}</Button></>}><div className="binding-target"><span className="component-icon component-icon--cli"><ArchiveRestore size={16} /></span><div><strong>{t.backend(restoreBackup.name)}</strong><span>{displayPath(restoreBackup.originalPath)}</span></div></div><p className="modal-copy">{t('settings.restore.body')}</p><p className="library-path">{t('settings.restore.path',{path:displayPath(restoreBackup.path)})}</p>{restoreBackup.locations && <ul className="skill-lifecycle__locations">{restoreBackup.locations.map(l => <li key={l.path}><strong>{t.backend(l.label)}</strong><span>{displayPath(l.path)}</span></li>)}</ul>}</Modal>}
  </>;
}

/// Saving writes through the normal settings path, so an unsupported value is
/// normalized by the core rather than trusted from the select.
function LanguageSettings({ settings, onSave }: { settings: Settings; onSave: (settings: Settings, message?: string) => Promise<void> }) {
  const t = useT();
  const [busy, setBusy] = useState(false);
  const change = (language: string) => { setBusy(true); void onSave({ ...settings, language }, makeTranslate(resolveLang(language))('settings.language.saved')).finally(() => setBusy(false)); };
  return <section className="settings-card"><h2>{t('settings.language')}</h2><p>{t('settings.language.description')}</p><label className="field"><span>{t('settings.language')}</span><select value={settings.language || ''} disabled={busy} onChange={event => change(event.target.value)}><option value="">{t('settings.language.system')}</option>{LANGUAGES.map(item => <option key={item.id} value={item.id}>{item.label}</option>)}</select></label></section>;
}


function ScanRootDialog({ targets, onClose, onSave }: { targets:AgentTarget[]; onClose: () => void; onSave: (root: ScanRoot) => void }) {
  const t: Translate = useT();
  const scopes = scopeLabels(t);
  const [agent, setAgent] = useState<AgentKey>((targets[0]?.id||'shared') as AgentKey);
  const [scope, setScope] = useState<DeploymentScope>('global');
  const [profile, setProfile] = useState('');
  const [path, setPath] = useState('');
  const submit = () => { if (!path.trim()) return; onSave({ id: makeId('root'), agent, scope, profile: scope === 'profile' ? profile.trim() || undefined : undefined, path: path.trim() }); };
  return <Modal title={t('settings.scanRoot.title')} onClose={onClose} footer={<><Button variant="secondary" onClick={onClose}>{t('common.cancel')}</Button><Button variant="primary" disabled={!path.trim()} onClick={submit} icon={<FolderOpen size={15} />}>{t('settings.scanRoot.add')}</Button></>}><div className="form-grid"><label className="field"><span>{t('settings.scanRoot.agent')}</span><select value={agent} onChange={(event) => setAgent(event.target.value as AgentKey)}>{targets.map(target=><option key={target.id} value={target.id}>{target.name}</option>)}<option value="shared">{t('settings.scanRoot.shared')}</option></select></label><label className="field"><span>{t('settings.scanRoot.scope')}</span><select value={scope} onChange={(event) => setScope(event.target.value as DeploymentScope)}>{(['global','project','profile'] as DeploymentScope[]).map(id=><option key={id} value={id}>{scopes[id]}</option>)}</select></label>{scope === 'profile' && <label className="field field--full"><span>{t('settings.scanRoot.profileName')}</span><input value={profile} onChange={(event) => setProfile(event.target.value)} placeholder={t('settings.scanRoot.profilePlaceholder')} /></label>}<label className="field field--full"><span>{t('settings.scanRoot.path')}</span><div className="input-with-icon"><FolderOpen size={14} /><input autoFocus value={path} onChange={(event) => setPath(event.target.value)} placeholder="C:\\Users\\…\\skills" /></div></label></div></Modal>;
}
