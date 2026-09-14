import { beforeEach, expect, it, vi } from 'vitest';
import { render, screen, within, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { RepositoryBookmarks, type RepositoryBookmark } from './RepositoryBookmarks';
import * as api from './api';
beforeEach(()=>vi.restoreAllMocks());
const bookmark:RepositoryBookmark={id:'repo',name:'Repository',url:'https://github.com/example/repo',notes:'记录',status:'interested',archived:false};
it('shows linked archived records together with standalone local projects in one list',async()=>{
  vi.spyOn(api,'dispatch').mockResolvedValue({items:[{...bookmark,archived:true,projectId:'project'}]} as never);
  const projects=[{id:'project',name:'本机项目',path:'C:/fixture',gitTrusted:false,archived:true,createdAt:'',updatedAt:''},{id:'standalone',name:'独立目录',path:'C:/other',gitTrusted:false,archived:true,createdAt:'',updatedAt:''}];
  render(<RepositoryBookmarks projects={projects} dataScope="fixture" archived/>);
  await screen.findByRole('link',{name:'Repository'});
  const cards=screen.getAllByRole('article');expect(cards).toHaveLength(2);
  expect(cards.find(c=>c.textContent?.includes('Repository'))).toHaveTextContent('本机项目 · 本机项目');
  expect(cards.find(c=>c.textContent?.includes('Repository'))).toHaveTextContent('C:\\fixture');
  expect(screen.getAllByRole('button',{name:'取消归档'})).toHaveLength(2);
});
it('saves manual status and project association without source inspection',async()=>{
  const dispatch=vi.spyOn(api,'dispatch').mockImplementation(async (...[method]) =>(method==='bookmarks.list'||method==='bookmarks.sync')?{items:[bookmark]} as never:{} as never);
  render(<RepositoryBookmarks projects={[{id:'project',name:'本机项目',path:'C:/fixture',gitTrusted:false,archived:false,createdAt:'',updatedAt:''}]} dataScope="fixture"/>);
  await userEvent.click(await screen.findByRole('button',{name:'编辑收藏 Repository'}));
  const dialog=within(screen.getByRole('dialog'));
  await userEvent.selectOptions(dialog.getByLabelText('关联本机项目（选填）'),'project');
  expect(dialog.getByLabelText('收藏状态')).toHaveValue('in_use');
  expect(dialog.getByLabelText('收藏状态')).toBeDisabled();
  await userEvent.click(dialog.getByRole('button',{name:'保存收藏'}));
  await waitFor(()=>expect(dispatch).toHaveBeenCalledWith('bookmarks.save',{bookmark:{...bookmark,status:'in_use',projectId:'project'}}));
  expect(dispatch.mock.calls.every(([method])=>method.startsWith('bookmarks.'))).toBe(true);
});
it('archives a bookmark while preserving its manual status and reports failures',async()=>{
  const dispatch=vi.spyOn(api,'dispatch').mockImplementation(async (...[method]) =>{if((method==='bookmarks.list'||method==='bookmarks.sync'))return {items:[bookmark]} as never;throw Error('无法保存');});
  render(<RepositoryBookmarks projects={[]} dataScope="fixture"/>);
  await userEvent.click(await screen.findByRole('button',{name:'归档'}));
  expect(dispatch).toHaveBeenCalledWith('bookmarks.save',{bookmark:{...bookmark,archived:true}});
  expect(await screen.findByRole('alert')).toHaveTextContent('无法保存');
  expect(screen.getByRole('link',{name:'Repository'})).toBeVisible();
});

it('saves tags, filters by tag, and opens the card link without triggering it from edit',async()=>{
  const dispatch=vi.spyOn(api,'dispatch').mockImplementation(async (...[method]) =>(method==='bookmarks.list'||method==='bookmarks.sync')?{items:[{...bookmark,tags:['开发']},{...bookmark,id:'other',name:'Other',url:'https://github.com/example/other',tags:['写作']}]} as never:{} as never);
  const open=vi.spyOn(api,'openExternal').mockResolvedValue(undefined);
  render(<RepositoryBookmarks projects={[]} dataScope="fixture"/>);
  await userEvent.selectOptions(await screen.findByLabelText('收藏标签筛选'),'开发');
  expect(screen.queryByRole('link',{name:'Other'})).not.toBeInTheDocument();
  await userEvent.click(screen.getByRole('link',{name:'Repository'}));
  expect(open).toHaveBeenCalledWith(bookmark.url);
  await userEvent.click(screen.getByRole('button',{name:'编辑收藏 Repository'}));
  expect(open).toHaveBeenCalledTimes(1);
  const dialog=within(screen.getByRole('dialog'));
  await userEvent.type(dialog.getByLabelText('标签，用逗号分隔'),'，工具');
  await userEvent.click(dialog.getByRole('button',{name:'保存收藏'}));
  expect(dispatch).toHaveBeenCalledWith('bookmarks.save',{bookmark:{...bookmark,tags:['开发','工具']}});
});
