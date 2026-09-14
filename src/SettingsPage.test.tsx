import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, expect, it, vi } from 'vitest';
import * as api from './api';
import { SettingsPage } from './SettingsPage';
import type { Settings } from './types';
beforeEach(()=>vi.restoreAllMocks());
const targets=[{id:'codex',name:'Codex',globalPath:'C:/Users/x/.codex/skills',projectPath:'.agents/skills',enabled:true,available:true},{id:'hermes',name:'Hermes',globalPath:'C:/Users/x/.hermes/skills',projectPath:'',enabled:true,available:false}];
const settings:Settings={scanRoots:[{id:'r1',agent:'codex',scope:'global',path:'D:/shared/skills'}],executables:{codex:'',claude:'',dsh:''}};
const page=(onSave=vi.fn().mockResolvedValue(undefined))=>{vi.spyOn(api,'dispatch').mockImplementation(async (...[method]) =>(method==='targets.list'?{targets}:{}) as never);render(<SettingsPage notify={vi.fn()} settings={settings} backups={[]} operations={[]} onSave={onSave} onRestore={vi.fn()}/>);return onSave;};
it('keeps install locations primary, other lookup locations visible, and program paths only where detection failed',async()=>{
  page();
  expect(screen.getByRole('tablist',{name:'设置分组'}).textContent).toBe('Agent更新与网络数据关于');
  expect(await screen.findByRole('heading',{name:'Skill 安装位置'})).toBeVisible();
  expect(screen.getByText(/所有项目位置对这台电脑上的全部项目生效/)).toBeVisible();
  const program=screen.getByRole('heading',{name:'应用程序位置'}).closest('section')!;
  expect(within(program).getByLabelText('Hermes 程序位置')).toHaveValue('');
  expect(within(program).queryByLabelText('Codex 程序位置')).not.toBeInTheDocument();
  const advanced=screen.getByText('其他 Skill 查找位置').closest('section')!;
  expect(within(advanced).getByText('D:\\shared\\skills')).toBeVisible();
});
it('saves a program path for the undetected Agent without touching install locations',async()=>{
  const onSave=page();
  await userEvent.type(await screen.findByLabelText('Hermes 程序位置'),'C:/apps/hermes.exe');
  await userEvent.click(screen.getByRole('button',{name:'保存应用程序位置'}));
  await waitFor(()=>expect(onSave).toHaveBeenCalledWith({...settings,executables:{...settings.executables,hermes:'C:/apps/hermes.exe'}}));
});
it('adds a read-only lookup location from the advanced section',async()=>{
  const onSave=page();

  await userEvent.click(screen.getByRole('button',{name:'添加查找位置'}));
  await userEvent.type(screen.getByLabelText('目录路径'),'E:/extra/skills');
  await userEvent.click(screen.getByRole('button',{name:'添加位置'}));
  await userEvent.click(screen.getByRole('button',{name:'保存查找位置'}));
  await waitFor(()=>expect(onSave).toHaveBeenCalledTimes(1));
  expect(onSave.mock.calls[0][0].scanRoots.map((r:{path:string})=>r.path)).toEqual(['D:/shared/skills','E:/extra/skills']);
});
