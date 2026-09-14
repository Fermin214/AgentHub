import {render,screen,waitFor} from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import {it,expect,vi} from 'vitest';
import * as api from './api';
import {BackupRetentionSettings} from './PreferencesPanel';
it('previews deletions before save, reports failures and allows retrying the same limit',async()=>{
 let saved=false,attempt=0;
 const dispatch=vi.spyOn(api,'dispatch').mockImplementation(async (...[method,args]) =>{
  if(method==='maintenance.get')return {maxBackups:3} as never;
  if(method==='maintenance.preview')return {maxBackups:args?.maxBackups,pruneCount:saved?0:2,protectedCount:1} as never;
  if(method==='maintenance.save'){attempt++;saved=attempt>1;return {maxBackups:3,retention:{removedCount:saved?2:0,failures:saved?[]:[{message:'文件占用'}]}} as never;}
  throw Error(method);
 });
 try{
  render(<BackupRetentionSettings notify={vi.fn()}/>);
  expect(await screen.findByText(/保存后将清理 2 份/)).toBeVisible();
  expect(dispatch).not.toHaveBeenCalledWith('maintenance.save',expect.anything());
  await userEvent.click(screen.getByRole('button',{name:'保存并整理备份'}));
  expect(await screen.findByRole('alert')).toHaveTextContent('文件占用');
  expect(screen.getByRole('button',{name:'保存并整理备份'})).toBeEnabled();
  await userEvent.click(screen.getByRole('button',{name:'保存并整理备份'}));
  await waitFor(()=>expect(screen.getByRole('button',{name:'保存并整理备份'})).toBeDisabled());
  expect(screen.getByText(/保存后将清理 0 份/)).toBeVisible();expect(attempt).toBe(2);
 }finally{dispatch.mockRestore();}
});
