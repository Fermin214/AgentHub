import { useEffect, useRef, useState } from 'react';
import * as api from './api';
import { SkillFileDiff } from './SkillFileDiff';
import { Button, Modal, type Notify } from './ui';
import { useT } from './i18n';
import type { SkillUpdateController } from './useSkillUpdates';
import type { ChangeResult, Skill, SkillChangePlan, UpdateCheck } from './types';

export function SkillUpdateDialog({ skill, controller, refresh, notify, onClose }: {
  skill: Skill; controller: SkillUpdateController; refresh: () => Promise<void>; notify: Notify; onClose: () => void;
}) {
  const t = useT();
  const saved = controller.checks[skill.id];
  const [comparison, setComparison] = useState<UpdateCheck | undefined>(saved);
  const [selected, setSelected] = useState<string[]>(() => (saved?.locations || []).filter(l => l.differences.length).map(l => l.id));
  const [retainBackup, setRetainBackup] = useState(true);
  const [policyReady, setPolicyReady] = useState(false);
  const [busy, setBusy] = useState(false);
  const [finished, setFinished] = useState(false);
  const [error, setError] = useState('');
  const working = useRef(false);
  const installComparison = (value: UpdateCheck) => {
    setComparison(value);
    setSelected((value.locations || []).filter(l => l.differences.length).map(l => l.id));
  };
  useEffect(() => {
    let live = true;
    void api.dispatch('maintenance.get').then(policy => {
      if (live) { setRetainBackup(policy.retainUpdateBackup !== false); setPolicyReady(true); }
    }).catch(cause => { if (live) setError(String(cause)); });
    return () => { live = false; };
  }, [skill.id]);
  const work = async (action: () => Promise<void>) => {
    if (working.current) return;
    working.current = true; setBusy(true); setError('');
    try { await action(); } catch (cause) { setError(String(cause)); } finally { working.current = false; setBusy(false); }
  };
  const execute = () => work(async () => {
    if (!comparison?.checkId || finished) return;
    let plan: SkillChangePlan | undefined;
    try {
      plan = await api.dispatch('skills.update.preview', { skillId: skill.id, checkId: comparison.checkId, locationIds: selected, retainBackup });
      if (!plan.canExecute) throw Error(plan.blockedReason || plan.summary);
      const result = await api.dispatch('skills.update', { planId: plan.id, confirmed: true });
      setFinished(true);
      notify(result.summary, result.status === 'succeeded' ? 'success' : 'error');
      await refresh();
      if (result.status === 'succeeded') onClose(); else setError(result.summary);
    } finally {
      if (plan) await api.dispatch('skills.cancel', { planId: plan.id }).catch(() => {});
    }
  });
  return <Modal title={t('update.title', { name: skill.name })} wide onClose={busy ? () => {} : onClose} footer={<>
    <Button disabled={busy} onClick={onClose}>{finished ? t('common.close') : t('common.cancel')}</Button>
    {!finished && <><Button disabled={busy} onClick={() => void work(async () => installComparison(await controller.runCheck(skill)))}>{t('update.recheck')}</Button><Button variant="primary" loading={busy} disabled={!selected.length || !comparison?.checkId || !policyReady || comparison.needsRefresh} onClick={() => void execute()}>{t('update.confirm')}</Button></>}
  </>}>
    {comparison ? <><p className="form-help">{t.backend(comparison.message)}{comparison.checkedAt && t('update.checkedPrefix', { time: t.dateTime(comparison.checkedAt) })}</p>{comparison.needsRefresh && <p role="status">{t('update.stale')}</p>}<SkillFileDiff comparison={comparison} selectedLocations={selected} onSelection={setSelected} disabled={busy || finished}/><label><input type="checkbox" checked={retainBackup} disabled={busy || finished || !policyReady} onChange={e => setRetainBackup(e.target.checked)}/>{t('update.retainBackup')}</label><p className="form-help">{t('update.locationCount', { n: selected.length })}</p></> : <p role="status">{t('update.noResult')}</p>}
    {error && <p role="alert" className="text-error">{t.backend(error)}</p>}
  </Modal>;
}
