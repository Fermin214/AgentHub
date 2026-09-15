import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import * as api from './api';
import { useSourceInspection } from './useSourceInspection';
const source = { kind: 'git' as const, locator: 'https://github.com/example/repo' };
const snapshot = { inspectionId: 'snapshot', source, candidates: [] };
beforeEach(() => vi.restoreAllMocks());

it('releases the reservation when dispatch fails before inspection starts', async () => {
  const dispatch = vi.spyOn(api, 'dispatch').mockImplementation(async (...[method]) => {
    if (method === 'sources.begin') return { requestId: 'request' } as never;
    if (method === 'sources.inspect') throw new Error('transport unavailable');
    return {} as never;
  });
  const { result } = renderHook(useSourceInspection);
  await act(async () => { await expect(result.current.run(source)).rejects.toThrow('transport unavailable'); });
  expect(dispatch).toHaveBeenCalledWith('sources.cancel', { requestId: 'request' });
  expect(result.current.active).toBe(false);
});

it('cancels before begin returns without launching an inspection', async () => {
  let finish!: (value: { requestId: string }) => void;
  const begin = new Promise<{ requestId: string }>(r => { finish = r; });
  const dispatch = vi.spyOn(api, 'dispatch').mockImplementation(async (...[method]) => method === 'sources.begin' ? await begin as never : {} as never);
  const { result } = renderHook(useSourceInspection);
  let outcome!: Promise<unknown>;
  act(() => { outcome = result.current.run(source).catch(e => String(e)); });
  await act(() => result.current.cancel());
  await act(async () => { finish({ requestId: 'request' }); await outcome; });
  expect(dispatch).toHaveBeenCalledWith('sources.cancel', { requestId: 'request' });
  expect(dispatch.mock.calls.some(([method]) => method === 'sources.inspect')).toBe(false);
  expect(result.current.active).toBe(false);
});

it('blocks reentry, releases a late success after cancellation, then permits retry', async () => {
  let finish!: (value: typeof snapshot) => void;
  const pending = new Promise<typeof snapshot>(r => { finish = r; });
  const dispatch = vi.spyOn(api, 'dispatch').mockImplementation(async (...[method]) => method === 'sources.begin' ? { requestId: 'request' } as never : method === 'sources.inspect' ? await pending as never : {} as never);
  const { result } = renderHook(useSourceInspection);
  let outcome!: Promise<unknown>;
  act(() => { outcome = result.current.run(source).catch(e => String(e)); });
  await waitFor(() => expect(dispatch).toHaveBeenCalledWith('sources.inspect', { source, requestId: 'request' }));
  await expect(result.current.run(source)).rejects.toThrow('SOURCE_BUSY');
  await act(() => result.current.cancel());
  await act(async () => { finish(snapshot); await outcome; });
  expect(dispatch).toHaveBeenCalledWith('sources.release', { inspectionId: 'snapshot' });
  expect(await outcome).toContain('SOURCE_CANCELLED');
  await act(async () => { expect(await result.current.run(source)).toEqual(snapshot); });
});
