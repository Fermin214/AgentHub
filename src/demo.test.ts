import { expect, it } from 'vitest';
import { dispatchDemo, previewSnapshot } from './demo';
it('serves independent static samples and refuses writes',async()=>{
  const before=await dispatchDemo('snapshot');before.prompts[0].title='changed';
  expect((await dispatchDemo('snapshot')).prompts[0].title).not.toBe('changed');
  await expect(dispatchDemo('prompts.save',{prompt:{...previewSnapshot.prompts[0],title:'new'}})).rejects.toThrow('桌面应用');
  await expect(dispatchDemo('skills.install',{planId:'id',confirmed:true})).rejects.toThrow('桌面应用');
});
