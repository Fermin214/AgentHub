import { beforeEach, expect, it, vi } from 'vitest';
import { render, screen, within, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import * as api from './api';
import { ProjectsPage } from './ProjectsPage';
beforeEach(()=>vi.restoreAllMocks());
it('uses the header action for the active tab and has no add action in the archive',async()=>{
  vi.spyOn(api,'dispatch').mockResolvedValue({items:[]} as never);
  render(<ProjectsPage projects={[]} dataScope="test" refresh={vi.fn()} onManageSkills={vi.fn()}/>);
  expect(screen.getByRole('button',{name:'添加本机项目'})).toBeVisible();
  await userEvent.click(screen.getByRole('tab',{name:'仓库收藏'}));
  expect(screen.queryByRole('button',{name:'添加本机项目'})).not.toBeInTheDocument();
  expect(screen.getAllByRole('button',{name:'添加收藏'})).toHaveLength(1);
  await userEvent.click(screen.getByRole('button',{name:'添加收藏'}));
  const dialog=within(await screen.findByRole('dialog'));
  await userEvent.click(dialog.getByRole('button',{name:'关闭'}));
  await userEvent.click(screen.getByRole('tab',{name:'已归档'}));
  expect(screen.queryByRole('button',{name:/添加/})).not.toBeInTheDocument();
  await userEvent.click(screen.getByRole('tab',{name:'仓库收藏'}));
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
});

const project={id:'p',name:'Local project',path:'C:/projects/local',gitTrusted:false,archived:false,createdAt:'',updatedAt:''};
it('archives and deletes only the chosen project record with the domain projectId',async()=>{
  const dispatch=vi.spyOn(api,'dispatch').mockImplementation(async (...[method]) =>method==='projects.status'?{update:null} as never:{} as never);
  const refresh=vi.fn().mockResolvedValue(undefined);render(<ProjectsPage projects={[project]} dataScope="fixture" refresh={refresh} onManageSkills={vi.fn()}/>);
  await userEvent.click(screen.getByRole('button',{name:'归档'}));await waitFor(()=>expect(dispatch).toHaveBeenCalledWith('projects.archive',{projectId:'p',archived:true}));
  await userEvent.click(screen.getByRole('button',{name:'删除记录'}));expect(screen.getByRole('dialog')).toHaveTextContent('本机目录和文件保留');
  expect(dispatch.mock.calls.some(([m])=>m==='projects.delete')).toBe(false);await userEvent.click(screen.getByRole('button',{name:'确认删除记录'}));await waitFor(()=>expect(dispatch).toHaveBeenCalledWith('projects.delete',{projectId:'p'}));
});

it('requires an understandable trust confirmation before enabling Git and allows revocation',async()=>{
  const dispatch=vi.spyOn(api,'dispatch').mockResolvedValue({update:null} as never);
  const props={projects:[project],dataScope:'fixture',refresh:vi.fn().mockResolvedValue(undefined),onManageSkills:vi.fn()};
  const view=render(<ProjectsPage {...props}/>);
  expect(screen.queryByRole('button',{name:/检查上游/})).not.toBeInTheDocument();
  await userEvent.click(screen.getByRole('button',{name:'启用 Git 检查'}));
  expect(dispatch.mock.calls.some(([m])=>m==='projects.trust')).toBe(false);
  const dialog=within(screen.getByRole('dialog'));
  expect(dialog.getByText(/SSH、凭据助手/)).toBeVisible();
  expect(dialog.getByText(/自动检查及仓库链接补全/)).toBeVisible();
  await userEvent.click(dialog.getByRole('button',{name:'我信任此项目，启用 Git'}));
  await waitFor(()=>expect(dispatch).toHaveBeenCalledWith('projects.trust',{projectId:'p',path:project.path,trusted:true,confirmed:true}));
  view.rerender(<ProjectsPage {...props} projects={[{...project,gitTrusted:true}]}/>);
  await userEvent.click(screen.getByRole('button',{name:'撤销 Git 信任'}));
  await waitFor(()=>expect(dispatch).toHaveBeenCalledWith('projects.trust',{projectId:'p',path:project.path,trusted:false}));
});
