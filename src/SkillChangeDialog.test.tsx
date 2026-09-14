import { expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import * as api from './api';
import { SkillChangeDialog } from './SkillChangeDialog';
import type { SkillChangePlan } from './types';
it('surfaces failed execution and prevents reusing an already-consumed plan',async()=>{
  const dispatch=vi.spyOn(api,'dispatch').mockImplementation(async (...[method]) =>method==='targets.list'?{targets:[]} as never:{status:'failed',summary:'已回滚，请重新检查'} as never);
  const plan={id:'plan',skillId:'s',action:'update',summary:'更新',canExecute:true,createdAt:'',locations:[]} as SkillChangePlan;
  const complete=vi.fn().mockResolvedValue(undefined);render(<SkillChangeDialog plan={plan} onClose={vi.fn()} onComplete={complete}/>);
  await userEvent.click(screen.getByRole('button',{name:'确认应用更新'}));expect(await screen.findByRole('alert')).toHaveTextContent('已回滚');await waitFor(()=>expect(screen.getByRole('button',{name:'确认应用更新'})).toBeDisabled());
  expect(complete).toHaveBeenCalledTimes(1);dispatch.mockRestore();
});
