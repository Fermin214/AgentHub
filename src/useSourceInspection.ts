import { useEffect, useRef, useState } from 'react';
import * as api from './api';
import type { Source } from './types';
import type { SourceProgress } from './contracts';

export function useSourceInspection() {
  const current = useRef<{ cancelled: boolean; id?: string }>();
  const mounted = useRef(true);
  const [progress, setProgress] = useState<SourceProgress>();
  const cancel = async () => {
    const operation = current.current;
    if (!operation) return;
    operation.cancelled = true;
    if (operation.id) await api.dispatch('sources.cancel', { requestId: operation.id });
  };
  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; void cancel().catch(() => {}); };
  }, []);
  const run = async (source: Source) => {
    if (current.current) throw new Error('SOURCE_BUSY');
    const operation: { cancelled: boolean; id?: string } = { cancelled: false };
    current.current = operation;
    setProgress({ stage: 'preparing', elapsedSeconds: 0, attempt: 0 });
    let timer: ReturnType<typeof setTimeout> | undefined;
    try {
      const { requestId } = await api.dispatch('sources.begin');
      if (!requestId) throw new Error('Source inspection did not return a request ID');
      operation.id = requestId;
      if (operation.cancelled) {
        await api.dispatch('sources.cancel', { requestId });
        throw new Error('SOURCE_CANCELLED');
      }
      const poll = async () => {
        try {
          const value = await api.dispatch('sources.status', { requestId });
          if (mounted.current && current.current === operation) setProgress(value);
        } catch { /* The main request owns errors; progress polling must not mask them. */ }
        if (current.current === operation) timer = setTimeout(() => void poll(), 250);
      };
      timer = setTimeout(() => void poll(), 250);
      const result = await api.dispatch('sources.inspect', { source, requestId });
      if (operation.cancelled || !mounted.current) {
        await api.dispatch('sources.release', { inspectionId: result.inspectionId });
        throw new Error('SOURCE_CANCELLED');
      }
      return result;
    } catch (error) {
      // Release a reservation even if the transport failed before inspection began.
      if (operation.id) await api.dispatch('sources.cancel', { requestId: operation.id }).catch(() => {});
      throw error;
    } finally {
      clearTimeout(timer);
      if (current.current === operation) current.current = undefined;
      if (mounted.current) setProgress(undefined);
    }
  };
  return { run, cancel, progress, active: Boolean(progress) };
}
