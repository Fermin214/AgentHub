import codex from './assets/agents/codex.svg';
import claude from './assets/agents/claudecode.svg';
import dsh from './assets/agents/deepseek.svg';

const icons: Record<string, string> = { codex, claude, dsh };
export function AgentIcon({ agent, name, size = 22 }: { agent: string; name: string; size?: number }) {
  const brand=icons[agent]?agent:Object.keys(icons).find(key=>name.toLowerCase().startsWith(key));
  return brand
    ? <img className="agent-brand-icon" src={icons[brand]} width={size} height={size} alt="" aria-hidden="true"/>
    : <span className="agent-brand-fallback" aria-hidden="true">{name.slice(0, 2)}</span>;
}
