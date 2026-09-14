import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import * as api from './api';
import { isCheckDue, useAutomaticUpdates } from './useAutomaticUpdates';
const now=new Date('2026-09-10T12:00:00Z');
const policy={automaticChecks:true,intervalHours:24};
const deferred=<T,>()=>{let resolve!:(value:T)=>void;const promise=new Promise<T>(r=>{resolve=r;});return {promise,resolve};};
beforeEach(()=>{vi.useFakeTimers();vi.setSystemTime(now);vi.restoreAllMocks();});
afterEach(()=>vi.useRealTimers());
describe('automatic update scheduling',()=>{
  it('respects disabled checks and measures the interval from the last attempt',()=>{
    expect(isCheckDue({...policy,automaticChecks:false})).toBe(false);
    expect(isCheckDue({...policy,lastAttemptAt:'2026-09-09T12:00:01Z'})).toBe(false);
    expect(isCheckDue({...policy,lastAttemptAt:'2026-09-09T12:00:00Z'})).toBe(true);
  });
  it('throttles a failed run using the persisted attempt and clears timers on unmount',async()=>{
    const get=vi.spyOn(api,'dispatch').mockResolvedValueOnce(policy).mockResolvedValue({...policy,lastAttemptAt:now.toISOString()});
    const check=vi.fn().mockRejectedValue(new Error('network unavailable'));
    const view=renderHook(()=>useAutomaticUpdates('isolated',false,check));
    await act(async()=>{await vi.advanceTimersByTimeAsync(1500);});
    expect(check).toHaveBeenCalledTimes(1);
    await act(async()=>{await vi.advanceTimersByTimeAsync(60000);});
    expect(get).toHaveBeenCalledTimes(2);
    expect(check).toHaveBeenCalledTimes(1);
    view.unmount();expect(vi.getTimerCount()).toBe(0);
  });
  it('does not overlap work while busy or while a prior policy read is pending',async()=>{
    const pending=deferred<typeof policy>();
    const get=vi.spyOn(api,'dispatch').mockReturnValue(pending.promise);
    const check=vi.fn().mockResolvedValue(undefined);
    const view=renderHook(({busy})=>useAutomaticUpdates('isolated',busy,check),{initialProps:{busy:true}});
    await act(async()=>{await vi.advanceTimersByTimeAsync(1500);});expect(get).not.toHaveBeenCalled();
    view.rerender({busy:false});
    await act(async()=>{window.dispatchEvent(new Event('agenthub-maintenance-changed'));});
    await act(async()=>{window.dispatchEvent(new Event('agenthub-maintenance-changed'));await vi.advanceTimersByTimeAsync(60000);});
    expect(get).toHaveBeenCalledTimes(1);
    view.rerender({busy:true});
    await act(async()=>pending.resolve(policy));
    expect(check).not.toHaveBeenCalled();view.unmount();
  });
  it('discards an old data directory policy when the scope changes',async()=>{
    const old=deferred<typeof policy>();
    vi.spyOn(api,'dispatch').mockReturnValueOnce(old.promise).mockResolvedValue({...policy,automaticChecks:false});
    const check=vi.fn().mockResolvedValue(undefined);
    const view=renderHook(({scope})=>useAutomaticUpdates(scope,false,check),{initialProps:{scope:'old'}});
    await act(async()=>{await vi.advanceTimersByTimeAsync(1500);});
    view.rerender({scope:'new'});
    await act(async()=>old.resolve(policy));
    await act(async()=>{await vi.advanceTimersByTimeAsync(1500);});
    expect(check).not.toHaveBeenCalled();view.unmount();expect(vi.getTimerCount()).toBe(0);
  });
});
