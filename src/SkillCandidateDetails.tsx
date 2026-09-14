import { useT } from './i18n';
import type { SkillCandidate } from './types';

export function SkillCandidateDetails({candidate}:{candidate:SkillCandidate}) {
  const t=useT();
  const comparison=candidate.contentComparison;
  return <><small className="candidate-path">{t('candidate.path')}<code>{candidate.subpath||'./'}</code></small>{comparison&&<div className="candidate-comparison">{(['same','different','unknown'] as const).map(kind=>comparison[kind]?.length>0&&<small key={kind}>{t('candidate.'+kind)}{t.lang==='zh'?'：':': '}{comparison[kind].map(path=><code key={path}>{path||'./'}</code>)}</small>)}</div>}</>;
}
