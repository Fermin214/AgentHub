// Compiled by `npm run check`; deliberately never invoked at runtime.
import type { dispatch } from './api';
import type { dispatchDemo } from './demo';
import type { DispatchCall, Method, Response } from './contracts';
import type { ChangeResult, Snapshot } from './types';

export async function verifyDispatchTypes(call: typeof dispatch, preview: typeof dispatchDemo, arbitrary: string, method: 'skills.install.preview' | 'skills.remove.preview') {
  const snapshot: Snapshot = await call('snapshot');
  const result: ChangeResult = await call('skills.install', { planId: 'plan', confirmed: true });
  const files: Response<'skills.files'> = await call('skills.files', { skillId: 'skill' });
  await call('bookmarks.list');
  await call('bookmarks.list', { status: 'all' });
  await call('projects.trust', { projectId: 'project', path: 'C:/project', trusted: true, confirmed: true });
  await call('projects.trust', { projectId: 'project', path: 'C:/project', trusted: false });
  const pair: DispatchCall = ['skills.remove.preview', { deploymentId: 'deployment' }];
  await call(...pair);
  const methodSpecific: Response<'snapshot'> = await preview('snapshot');

  // @ts-expect-error Unknown methods have no string fallback.
  await call('skills.typo');
  // @ts-expect-error A computed arbitrary string cannot bypass the map.
  await call(arbitrary, {});
  // @ts-expect-error Callers cannot choose their own response type.
  await call<Snapshot>('snapshot');
  // @ts-expect-error A required request cannot be omitted.
  await call('skills.install');
  // @ts-expect-error Executing a plan requires explicit confirmation.
  await call('skills.install', { planId: 'plan', confirmed: false });
  // @ts-expect-error Arguments from another method do not match.
  await call('skills.install.preview', { deploymentId: 'deployment' });
  // @ts-expect-error Widening the method union cannot decouple method and payload.
  await call<Method>('skills.install.preview', { deploymentId: 'deployment' });
  // @ts-expect-error Dynamic preview methods still require a correlated tuple.
  await call(method, { deploymentId: 'deployment' });
  // @ts-expect-error Snapshot takes no mutation fields.
  await call('snapshot', { projectId: 'project' });
  // @ts-expect-error Misspelled request keys fail at the actual call site.
  await call('skills.read', { skillId: 'skill', filepath: 'SKILL.md' });
  // @ts-expect-error Trust cannot be enabled without explicit confirmation.
  await call('projects.trust', { projectId: 'project', path: 'C:/project', trusted: true });
  // @ts-expect-error Responses are fixed by their method.
  const unrelated: ChangeResult = await call('snapshot');
  // @ts-expect-error A read response does not expose arbitrary properties.
  files.missing;
  // @ts-expect-error The read-only preview uses the same request contract.
  await preview('skills.read', { skillId: 'skill' });
  // @ts-expect-error The read-only preview cannot specify an arbitrary response.
  await preview<Snapshot>('snapshot');
  return { snapshot, result, files, methodSpecific, unrelated };
}
