import { useMemo, useState } from 'react';
import { Clipboard, Star, Copy, Plus, LogOut, Pencil, Trash2, Save, FileJson, FileText, Info } from 'lucide-react';
import * as api from './api';
import type { Prompt, Snapshot } from './types';
import { useT } from './i18n';
import { Button, IconButton, EmptyState, Modal, PageHeader, nowIso, SearchField } from './ui';

/// An empty tag filter means "all tags"; a real tag can never be empty, so the
/// sentinel stays outside the translated text.
const ALL_TAGS = '';
export function PromptsPage({ prompts, notify, onSnapshot }: { prompts: Prompt[]; notify: (message: string, tone?: 'success' | 'error' | 'info') => void; onSnapshot: React.Dispatch<React.SetStateAction<Snapshot | null>> }) {
  const t = useT();
  const [search, setSearch] = useState('');
  const [tag, setTag] = useState(ALL_TAGS);
  const [sort,setSort]=useState('updated');
  const [editorPrompt, setEditorPrompt] = useState<Prompt | null | undefined>(undefined);
  const [preview, setPreview] = useState<Prompt>();
  const [exportOpen, setExportOpen] = useState(false);
  const [deletePrompt, setDeletePrompt] = useState<Prompt | null>(null);
  const [showFavorites, setShowFavorites] = useState(false);

  const tags = useMemo(() => Array.from(new Set(prompts.flatMap((prompt) => prompt.tags))), [prompts]);
  const filtered = useMemo(() => prompts.filter((prompt) => {
    const query = search.trim().toLowerCase();
    const matchesSearch = !query || `${prompt.title} ${prompt.purpose || ''} ${prompt.body} ${prompt.tags.join(' ')}`.toLowerCase().includes(query);
    return matchesSearch && (tag === ALL_TAGS || prompt.tags.includes(tag)) && (!showFavorites || prompt.favorite);
  }).sort((a,b)=>sort==='title'?t.compare(a.title,b.title):sort==='created'?b.createdAt.localeCompare(a.createdAt):b.updatedAt.localeCompare(a.updatedAt)), [prompts, search, tag, showFavorites, sort, t]);

  const copyPrompt = async (prompt: Prompt) => {
    try { await api.copyText(prompt.body); notify(t('prompts.copied',{title:prompt.title})); } catch (cause) { notify(cause instanceof Error ? cause.message : String(cause), 'error'); }
  };

  const save = async (draft: Prompt) => {
    try {
      const saved = await api.savePrompt(draft);
      onSnapshot((current) => current ? { ...current, prompts: current.prompts.some((item) => item.id === saved.id) ? current.prompts.map((item) => item.id === saved.id ? saved : item) : [saved, ...current.prompts] } : current);
      setEditorPrompt(undefined);
      notify(draft.id ? t('prompts.updated') : t('prompts.savedToast'));
    } catch (cause) { notify(cause instanceof Error ? cause.message : String(cause), 'error'); }
  };

  const remove = async () => {
    if (!deletePrompt) return;
    try { await api.deletePrompt(deletePrompt.id); onSnapshot((current) => current ? { ...current, prompts: current.prompts.filter((item) => item.id !== deletePrompt.id) } : current); notify(t('prompts.deleted')); setDeletePrompt(null); } catch (cause) { notify(cause instanceof Error ? cause.message : String(cause), 'error'); }
  };

  const toggleFavorite = async (prompt: Prompt) => { await save({ ...prompt, favorite: !prompt.favorite }); };

  return <>
    <PageHeader title={t('prompts.title')} description={t('prompts.description')} action={<><Button variant="primary" icon={<Plus size={16} />} onClick={() => setEditorPrompt(null)}>{t('prompts.add')}</Button><Button variant="secondary" icon={<LogOut size={15} />} onClick={() => setExportOpen(true)}>{t('prompts.export')}</Button></>} />
    <section className="toolbar"><SearchField label={t('prompts.search')} placeholder={t('prompts.searchPlaceholder')} value={search} onChange={setSearch}/><div className="toolbar__filters"><label>{t('common.tag')} <select aria-label={t('prompts.tagFilter')} value={tag} onChange={(event) => setTag(event.target.value)}><option value={ALL_TAGS}>{t('common.allTags')}</option>{tags.map((item) => <option key={item} value={item}>{item}</option>)}</select></label><button className={`filter-toggle ${showFavorites ? 'filter-toggle--active' : ''}`} onClick={() => setShowFavorites((value) => !value)}><Star size={15} /> {t('common.favoritesOnly')}</button></div></section>
    <div className="section-line"><span>{t('prompts.resultCount',{n:filtered.length})}</span><select className="prompt-sort" aria-label={t('prompts.sort')} value={sort} onChange={e=>setSort(e.target.value)}><option value="updated">{t('prompts.sort.updated')}</option><option value="created">{t('prompts.sort.created')}</option><option value="title">{t('prompts.sort.title')}</option></select></div>
    {filtered.length ? <div className="prompt-grid">{filtered.map((prompt) => <PromptCard key={prompt.id} prompt={prompt} onPreview={() => setPreview(prompt)} onEdit={() => setEditorPrompt(prompt)} onFavorite={() => void toggleFavorite(prompt)} onDelete={() => setDeletePrompt(prompt)} />)}</div> : prompts.length === 0 ? <EmptyState icon={Clipboard} title={t('prompts.empty.title')} description={t('prompts.empty.body')} action={<Button variant="primary" onClick={() => setEditorPrompt(null)} icon={<Plus size={15} />}>{t('prompts.empty.action')}</Button>} /> : <EmptyState icon={Clipboard} title={t('prompts.noMatch.title')} description={t('prompts.noMatch.body')} action={<Button variant="secondary" onClick={() => { setSearch(''); setTag(ALL_TAGS); setShowFavorites(false); }}>{t('prompts.noMatch.action')}</Button>} />}
    {preview && <Modal title={preview.title} wide onClose={() => setPreview(undefined)} footer={<><Button onClick={() => setPreview(undefined)}>{t('common.close')}</Button><Button variant="primary" icon={<Copy size={15}/>} onClick={() => void copyPrompt(preview)}>{t('prompts.preview.copy')}</Button></>}>{preview.purpose && <div className="prompt-purpose"><strong>{t('prompts.purpose')}</strong><p>{preview.purpose}</p></div>}<div className="prompt-preview-body">{preview.body}</div></Modal>}
    {editorPrompt !== undefined && <PromptEditorDialog prompt={editorPrompt} onClose={() => setEditorPrompt(undefined)} onSave={save} />}
    {exportOpen && <PromptExportDialog onClose={() => setExportOpen(false)} notify={notify} />}
    {deletePrompt && <Modal title={t('prompts.delete.title')} onClose={() => setDeletePrompt(null)} footer={<><Button variant="secondary" onClick={() => setDeletePrompt(null)}>{t('common.cancel')}</Button><Button variant="danger" onClick={() => void remove()} icon={<Trash2 size={15} />}>{t('prompts.delete.confirm')}</Button></>}><p className="modal-copy">{t('prompts.delete.body',{title:deletePrompt.title})}</p></Modal>}
  </>;
}

function PromptCard({ prompt, onPreview, onEdit, onFavorite, onDelete }: { prompt: Prompt; onPreview: () => void; onEdit: () => void; onFavorite: () => void; onDelete: () => void }) {
  const t = useT();
  return <article className="prompt-card"><div className="prompt-card__head"><button className="prompt-card__title" onClick={onPreview}><h3>{prompt.title}</h3></button><div className="prompt-card__actions"><IconButton label={prompt.favorite ? t('common.unfavorite') : t('common.favorite')} className={prompt.favorite ? 'icon-button--favorite' : ''} onClick={onFavorite}><Star size={16} fill={prompt.favorite ? 'currentColor' : 'none'} /></IconButton><IconButton label={t('prompts.editLabel')} onClick={onEdit}><Pencil size={15} /></IconButton><IconButton label={t('prompts.deleteLabel')} onClick={onDelete}><Trash2 size={15} /></IconButton></div></div><button className="prompt-card__copy" onClick={onPreview} aria-label={t('prompts.preview.label',{title:prompt.title})}>{prompt.purpose && <div className="prompt-card__purpose">{prompt.purpose}</div>}<p>{prompt.body}</p></button><div className="prompt-card__bottom"><div className="tag-list">{prompt.tags.map((tag) => <span key={tag}>{tag}</span>)}</div><time dateTime={prompt.updatedAt}>{t.relative(prompt.updatedAt)}</time></div></article>;
}

function PromptEditorDialog({ prompt, onClose, onSave }: { prompt: Prompt | null; onClose: () => void; onSave: (prompt: Prompt) => Promise<void> }) {
  const t=useT();
  const [title,setTitle]=useState(prompt?.title||'');
  const [titleEdited,setTitleEdited]=useState(Boolean(prompt));
  const [body,setBody]=useState(prompt?.body||'');
  const [purpose,setPurpose]=useState(prompt?.purpose||'');
  const [tags,setTags]=useState(prompt?.tags.join(t.lang==='zh'?'，':', ')||'');
  const [saving,setSaving]=useState(false);
  const chosenTitle=titleEdited?title:body.split(/\r?\n/).find(line=>line.trim())?.trim().slice(0,80)||'';
  const submit=async()=>{if(!body.trim()||!chosenTitle.trim()||saving)return;setSaving(true);try{await onSave({id:prompt?.id||'',title:chosenTitle.trim(),body,purpose:purpose.trim(),category:prompt?.category||'',tags:tags.split(/[,，]/).map(t=>t.trim()).filter(Boolean),favorite:prompt?.favorite||false,createdAt:prompt?.createdAt||nowIso(),updatedAt:nowIso()});}finally{setSaving(false);}};
  return <Modal title={prompt?t('prompts.editor.edit'):t('prompts.editor.add')} wide onClose={saving?()=>{}:onClose} footer={<><Button disabled={saving} onClick={onClose}>{t('common.cancel')}</Button><Button variant="primary" loading={saving} disabled={!body.trim()||!chosenTitle.trim()} icon={<Save size={15}/>} onClick={()=>void submit()}>{t('prompts.editor.save')}</Button></>}><div className="form-grid form-grid--prompt"><label className="field field--full"><span>{t('prompts.editor.title')}</span><input value={chosenTitle} onChange={e=>{setTitle(e.target.value);setTitleEdited(true);}} placeholder={t('prompts.editor.titlePlaceholder')}/></label><label className="field field--full"><span>{t('prompts.editor.purpose')}</span><textarea rows={2} value={purpose} onChange={e=>setPurpose(e.target.value)} placeholder={t('prompts.editor.purposePlaceholder')}/></label><label className="field field--full"><span>{t('prompts.editor.body')}</span><textarea autoFocus={!prompt} className="prompt-plain-editor" rows={12} value={body} onChange={e=>setBody(e.target.value)} placeholder={t('prompts.editor.bodyPlaceholder')} spellCheck={false}/></label><label className="field field--full"><span>{t('common.tagsComma')}</span><input value={tags} onChange={e=>setTags(e.target.value)}/></label></div></Modal>;
}

function PromptExportDialog({ onClose, notify }: { onClose: () => void; notify: (message: string, tone?: 'success' | 'error' | 'info') => void }) {
  const t = useT();
  const [format, setFormat] = useState<'json' | 'markdown'>('json');
  const [busy, setBusy] = useState(false);
  const submit = async () => { setBusy(true); try { const result = await api.exportPrompts(format); await api.saveExportFile(result.content, result.filename); notify(t('prompts.exportedToast',{filename:result.filename})); onClose(); } catch (cause) { notify(cause instanceof Error ? cause.message : String(cause), 'error'); } finally { setBusy(false); } };
  return <Modal title={t('prompts.exportTitle')} onClose={onClose} footer={<><Button variant="secondary" onClick={onClose}>{t('common.cancel')}</Button><Button variant="primary" loading={busy} onClick={() => void submit()} icon={<LogOut size={15} />}>{t('prompts.exportFile')}</Button></>}><div className="export-choice"><label className={`export-format ${format === 'json' ? 'export-format--active' : ''}`}><input type="radio" checked={format === 'json'} onChange={() => setFormat('json')} /><FileJson size={22} /><strong>JSON</strong><span>{t('prompts.exportJson')}</span></label><label className={`export-format ${format === 'markdown' ? 'export-format--active' : ''}`}><input type="radio" checked={format === 'markdown'} onChange={() => setFormat('markdown')} /><FileText size={22} /><strong>Markdown</strong><span>{t('prompts.exportMarkdown')}</span></label></div><p className="form-help"><Info size={14} /> {api.isTauriRuntime() ? t('prompts.exportHintDesktop') : t('prompts.exportHintBrowser')}</p></Modal>;
}
