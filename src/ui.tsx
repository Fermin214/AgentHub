import { useId, useLayoutEffect, useRef, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { LoaderCircle, Package, Search, X, CheckCircle2, AlertCircle, Info } from 'lucide-react';
import { useT } from './i18n';
export type Notify = (message: string, tone?: 'success' | 'error' | 'info') => void;
export const nowIso = () => new Date().toISOString();
export const makeId = (prefix: string) => `${prefix}-${Date.now()}-${Math.random().toString(16).slice(2, 8)}`;
export const copyValue = <T,>(value: T): T => JSON.parse(JSON.stringify(value)) as T;


export function Button({
  children,
  variant = 'secondary',
  size = 'md',
  icon,
  loading = false,
  disabled,
  className = '',
  ...props
}: React.ButtonHTMLAttributes<HTMLButtonElement> & { variant?: 'primary' | 'secondary' | 'quiet' | 'danger' | 'ghost'; size?: 'sm' | 'md'; icon?: ReactNode; loading?: boolean }) {
  return (
    <button className={`button button--${variant} button--${size} ${className}`} disabled={disabled || loading} {...props}>
      {loading ? <LoaderCircle className="spin" size={15} aria-hidden="true" /> : icon}
      {children}
    </button>
  );
}

export function IconButton({ label, children, className = '', ...props }: React.ButtonHTMLAttributes<HTMLButtonElement> & { label: string }) {
  return <button className={`icon-button ${className}`} aria-label={label} title={label} {...props}>{children}</button>;
}

export function Badge({ children, tone = 'neutral' }: { children: ReactNode; tone?: 'neutral' | 'blue' | 'green' | 'amber' | 'red' | 'purple' }) {
  return <span className={`badge badge--${tone}`}>{children}</span>;
}

export function SearchField({ label, placeholder, value, onChange }: { label: string; placeholder: string; value: string; onChange: (value: string) => void }) {
  const t = useT();
  const input = useRef<HTMLInputElement>(null);
  return <div className="search-box"><Search size={17} aria-hidden="true"/><input ref={input} aria-label={label} placeholder={placeholder} value={value} onChange={event => onChange(event.target.value)}/>{value && <IconButton label={t('common.clearSearch')} onClick={() => { onChange(''); input.current?.focus(); }}><X size={14} aria-hidden="true"/></IconButton>}</div>;
}

export function EmptyState({ icon: Icon = Package, title, description, action }: { icon?: typeof Package; title: string; description: string; action?: ReactNode }) {
  return <div className="empty-state"><div className="empty-state__icon"><Icon size={22} /></div><h3>{title}</h3><p>{description}</p>{action}</div>;
}

const modalStack: HTMLElement[] = [];
let modalTrigger: HTMLElement | null = null;
const backgroundAttributes = new Map<Element, { inert: string | null; hidden: string | null }>();
function updateModalBackground() {
  const active = modalStack.at(-1);
  for (const element of document.body.children) {
    if (element.getAttribute('role') === 'status') continue;
    if (!backgroundAttributes.has(element)) backgroundAttributes.set(element, { inert: element.getAttribute('inert'), hidden: element.getAttribute('aria-hidden') });
    const original = backgroundAttributes.get(element)!;
    if (active && element !== active) { element.setAttribute('inert', ''); element.setAttribute('aria-hidden', 'true'); }
    else {
      if (original.inert === null) element.removeAttribute('inert'); else element.setAttribute('inert', original.inert);
      if (original.hidden === null) element.removeAttribute('aria-hidden'); else element.setAttribute('aria-hidden', original.hidden);
    }
  }
  if (!active) backgroundAttributes.clear();
}

export function Modal({ title, titleAction, children, onClose, wide = false, footer }: { title: string; titleAction?: ReactNode; children: ReactNode; onClose: () => void; wide?: boolean; footer?: ReactNode }) {
  const t = useT();
  const titleId = useId();
  const dialog = useRef<HTMLElement>(null);
  const initialFocus = useRef<HTMLElement | null>(null);
  const close = useRef(onClose);
  close.current = onClose;
  // Remember the trigger before React mounts any autoFocus field in the dialog.
  const trigger = useRef(document.activeElement as HTMLElement | null);
  useLayoutEffect(() => {
    const panel = dialog.current!;
    const backdrop = panel.parentElement!;
    const focusable = () => Array.from(panel.querySelectorAll<HTMLElement>('button, a[href], input, select, textarea, summary, [tabindex]')).filter(element => element.tabIndex >= 0 && !element.matches(':disabled') && !element.closest('[hidden], [inert], [aria-hidden="true"]'));
    if (panel.contains(document.activeElement)) initialFocus.current = document.activeElement as HTMLElement;
    else (initialFocus.current || focusable()[0] || panel).focus();
    if (!modalStack.length) modalTrigger = trigger.current;
    modalStack.push(backdrop);
    updateModalBackground();
    const onKey = (event: KeyboardEvent) => {
      if (modalStack.at(-1) !== backdrop) return;
      if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); close.current(); }
      if (event.key !== 'Tab') return;
      const items = focusable();
      const first = items[0] || panel;
      const last = items.at(-1) || panel;
      if (!panel.contains(document.activeElement) || document.activeElement === panel || (event.shiftKey ? document.activeElement === first : document.activeElement === last)) {
        event.preventDefault();
        (event.shiftKey ? last : first).focus();
      }
    };
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('keydown', onKey);
      modalStack.splice(modalStack.indexOf(backdrop), 1);
      updateModalBackground();
      backgroundAttributes.delete(backdrop);
      const returnFocus = modalStack.length ? trigger.current : modalTrigger;
      if (!modalStack.length) modalTrigger = null;
      // React may unmount several nested dialogs in the same commit.
      queueMicrotask(() => { if (returnFocus?.isConnected && !returnFocus.closest('[inert]')) returnFocus.focus(); });
    };
  }, []);
  return createPortal(<div className="modal-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) onClose(); }}>
    <section ref={dialog} tabIndex={-1} className={`modal ${wide ? 'modal--wide' : ''}`} role="dialog" aria-modal="true" aria-labelledby={titleId}>
      <header className="modal__header">
        <div className={titleAction ? 'modal__title-row' : undefined}><h2 id={titleId}>{title}</h2>{titleAction}</div>
        <IconButton label={t('common.close')} onClick={onClose}><X size={18} /></IconButton>
      </header>
      <div className="modal__body">{children}</div>
      {footer && <footer className="modal__footer">{footer}</footer>}
    </section>
  </div>, document.body);
}

export function TabList({ id, label, value, items, onChange, disabled = false, className = 'settings-tabs' }: { id: string; label: string; value: string; items: Array<{ id: string; label: string }>; onChange: (id: string) => void; disabled?: boolean; className?: string }) {
  return <div className={className} role="tablist" aria-label={label}>{items.map((item, index) => <button key={item.id} id={`${id}-tab-${item.id}`} role="tab" aria-selected={value === item.id} aria-controls={`${id}-panel`} tabIndex={value === item.id ? 0 : -1} disabled={disabled} onClick={() => onChange(item.id)} onKeyDown={event => {
    const next = event.key === 'ArrowRight' ? (index + 1) % items.length : event.key === 'ArrowLeft' ? (index + items.length - 1) % items.length : event.key === 'Home' ? 0 : event.key === 'End' ? items.length - 1 : -1;
    if (next < 0) return;
    event.preventDefault();
    onChange(items[next].id);
    document.getElementById(`${id}-tab-${items[next].id}`)?.focus();
  }}>{item.label}</button>)}</div>;
}

export function Toast({ message, tone, onClose }: { message: string; tone: 'success' | 'error' | 'info'; onClose: () => void }) {
  const t = useT();
  const Icon = tone === 'success' ? CheckCircle2 : tone === 'error' ? AlertCircle : Info;
  return createPortal(<div className={`toast toast--${tone}`} role="status"><Icon size={17} /><span>{message}</span><IconButton label={t('common.closeHint')} onClick={onClose}><X size={14} /></IconButton></div>, document.body);
}

export function PageHeader({ title, description, action }: { title: string; description: string; action?: ReactNode }) {
  return <header className="page-header"><div><h1>{title}</h1><p>{description}</p></div>{action && <div className="page-header__actions">{action}</div>}</header>;
}
