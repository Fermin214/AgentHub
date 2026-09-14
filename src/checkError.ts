import type { Translate } from './i18n';
// Keep transport diagnostics available separately from the everyday status.
// The tail is a raw core message, so it goes through `t.backend` rather than a key.
export function checkError(message: string, t: Translate): string {
  if (/TLS|SSL|unexpected eof|tunnel.*(?:closed|handshake)/i.test(message)) return t('error.tls');
  if (/timed? out|超时|timeout/i.test(message)) return t('error.timeout');
  if (/could not resolve|failed to connect|connection reset|network|网络/i.test(message)) return t('error.network');
  return t.backend(message.replace(/^Error:\s*/, '').split(/\r?\n/)[0]).slice(0, 160);
}
