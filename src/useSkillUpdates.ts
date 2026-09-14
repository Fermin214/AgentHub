import { useEffect, useRef, useState } from 'react';
import * as api from './api';
import { checkError } from './checkError';
import { useT } from './i18n';
import type { Skill, Snapshot, UpdateCheck } from './types';
import type { Notify } from './ui';

type CheckOptions = { batchId?: string; useCached?: boolean };
export function useSkillUpdates(snapshot: Snapshot | null, refresh: () => Promise<void>, notify: Notify) {
  const t = useT();
  const [checks, setChecks] = useState<Record<string, UpdateCheck>>({});
  const [checkingIds, setCheckingIds] = useState<string[]>([]);
  const [checkProgress, setCheckProgress] = useState('');
  const [error, setError] = useState('');
  const pending = useRef(new Map<string, Promise<UpdateCheck>>());
  const batchRunning = useRef(false);
  const currentScope = useRef(snapshot?.dataScope);
  currentScope.current = snapshot?.dataScope;
  useEffect(() => {
    setChecks(Object.fromEntries((snapshot?.skillUpdates || []).filter(u => u.skillId).map(u => [u.skillId!, u])));
  }, [snapshot?.dataScope, snapshot?.skillUpdates]);

  const runCheck = (skill: Skill, options: CheckOptions = {}): Promise<UpdateCheck> => {
    const scope = currentScope.current;
    const key = `${scope}:${skill.id}`;
    const existing = pending.current.get(key);
    if (existing) return existing;
    setCheckingIds(v => [...v, skill.id]);
    const task = api.dispatch('skills.check', { skillId: skill.id, ...options }).then(async result => {
      if (scope === currentScope.current) {
        setChecks(v => ({ ...v, [skill.id]: result }));
        await refresh();
      }
      if (result.status === 'failed') throw new Error(result.message);
      return result;
    }).finally(() => {
      pending.current.delete(key);
      if (scope === currentScope.current) setCheckingIds(v => v.filter(id => id !== skill.id));
    });
    pending.current.set(key, task);
    return task;
  };
  const check = async (skill: Skill) => {
    setError('');
    try { await runCheck(skill); } catch (cause) { setError(t('check.skillFailed', { name: skill.name, message: checkError(String(cause), t) })); }
  };
  const checkAll = async () => {
    if (batchRunning.current || !snapshot) return;
    batchRunning.current = true;
    setError('');
    const eligible = snapshot.skills.filter(s => s.source.kind !== 'unknown' && s.source.locator);
    const failures: string[] = [];
    let batchId: string | undefined;
    let cursor = 0;
    let completed = 0;
    setCheckProgress(`0 / ${eligible.length}`);
    try {
      if (!eligible.length) return;
      const session = await api.dispatch('skills.checkBatch.start');
      if (!session.batchId) throw new Error(t('check.batchFailed'));
      batchId = session.batchId;
      const worker = async () => {
        while (cursor < eligible.length) {
          const skill = eligible[cursor++];
          try { await runCheck(skill, { batchId }); } catch (cause) { failures.push(skill.name); }
          finally { completed += 1; setCheckProgress(`${completed} / ${eligible.length}`); }
        }
      };
      await Promise.all(Array.from({ length: Math.min(3, eligible.length) }, worker));
      if (failures.length) setError(t('check.someFailed', { n: failures.length, names: failures.join(t.lang === 'zh' ? '、' : ', ') }));
      else notify(t('check.allDone', { n: eligible.length }) + (snapshot.skills.length > eligible.length ? t('check.noSourceSuffix', { n: snapshot.skills.length - eligible.length }) : ''), 'success');
    } catch (cause) { setError(String(cause)); }
    finally {
      if (batchId) {
        try { await api.dispatch('skills.checkBatch.finish', { batchId }); } catch (cause) { setError(String(cause)); }
      }
      batchRunning.current = false;
      setCheckProgress('');
    }
  };
  return { checks, checkingIds, checkProgress, error, check, checkAll, runCheck };
}
export type SkillUpdateController = ReturnType<typeof useSkillUpdates>;
