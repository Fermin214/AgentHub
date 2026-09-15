import { useT } from './i18n';
import type { SourceProgress } from './contracts';

export function SourceActivity({ progress }: { progress?: SourceProgress }) {
  const t = useT();
  if (!progress) return null;
  return <p role="status" className="form-help">{t('source.stage.' + progress.stage)} · {t('source.elapsed', { seconds: progress.elapsedSeconds })}{progress.attempt > 1 ? ` · ${t('source.attempt', { n: progress.attempt })}` : ''}</p>;
}
export function SourceFailure({ error }: { error: string }) {
  const t = useT();
  if (!error) return null;
  let detail = error.replace(/^Error:\s*/, '');
  let code = '';
  try { const value = JSON.parse(detail); if (typeof value.detail === 'string') detail = value.detail; code = value.code || ''; } catch { /* Plain core/CLI errors remain supported. */ }
  const raw = `${code} ${detail}`;
  const key = /SOURCE_CANCELLED/.test(raw) ? 'cancelled'
    : /SOURCE_BUSY/.test(raw) ? 'busy'
    : /SOURCE_CLEANUP_FAILED/.test(raw) ? 'cleanup'
    : /SOURCE_TIMEOUT|timed? out|timeout|超时/i.test(raw) ? 'timeout'
    : /repository not found|404/i.test(raw) ? 'repository'
    : /authentication|could not read username|401|403|permission denied/i.test(raw) ? 'auth'
    : /proxy|CONNECT tunnel|407/i.test(raw) ? 'proxy'
    : /TLS|SSL|certificate/i.test(raw) ? 'tls'
    : /could not resolve|failed to connect|connection reset|network/i.test(raw) ? 'network' : '';
  return <div role="alert" className="text-error"><p>{key ? t('source.error.' + key) : t.backend(detail)}</p>{key && <details className="check-diagnostic"><summary>{t('common.errorDetails')}</summary><pre>{detail}</pre></details>}</div>;
}
