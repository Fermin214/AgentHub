/** Display only: keep operation paths untouched, including UNC semantics. */
export function displayPath(value: string): string {
  value = value.replace(/\//g, '\\');
  if (value.startsWith('\\\\?\\UNC\\')) return '\\\\' + value.slice(8);
  return value.startsWith('\\\\?\\') ? value.slice(4) : value;
}
