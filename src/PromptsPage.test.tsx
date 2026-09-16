import { beforeEach,expect,it,vi } from 'vitest';
import { render,screen,waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { PromptsPage } from './PromptsPage';
import * as api from './api';
import type { Prompt, Snapshot } from './types';
const prompt:Prompt={id:'one',title:'A first',body:'Original body',tags:[],category:'',favorite:false,createdAt:'2026-09-01T00:00:00Z',updatedAt:'2026-09-01T00:00:00Z'};
beforeEach(()=>vi.restoreAllMocks());
it('sorts results by actual updated date or title independently of favorites',async()=>{
 render(<PromptsPage prompts={[{...prompt,favorite:true},{...prompt,id:'two',title:'Z latest',updatedAt:'2026-09-10T00:00:00Z'}]} notify={vi.fn()} onSnapshot={vi.fn()}/>);
 expect(screen.getAllByRole('button',{name:/查看 /})[0]).toHaveTextContent('Z latest');
 await userEvent.selectOptions(screen.getByLabelText('Prompt 排序'),'title');
 expect(screen.getAllByRole('button',{name:/查看 /})[0]).toHaveTextContent('A first');
});

it('keeps a newly saved Prompt in the list when prompts.save returns the Prompt contract',async()=>{
 const save=vi.spyOn(api,'savePrompt').mockImplementation(async (...[p]) =>({...p,id:'saved',title:'新条目'}));
 let current:Snapshot|null={dataScope:'fixture',prompts:[prompt],skills:[],deployments:[],projects:[],settings:{scanRoots:[],executables:{codex:'',claude:'',dsh:''}},operations:[]};
 render(<PromptsPage prompts={current.prompts} notify={vi.fn()} onSnapshot={update=>{current=typeof update==='function'?update(current):update}}/>);
 await userEvent.click(screen.getByRole('button',{name:'添加'}));
 await userEvent.type(screen.getByLabelText('正文'),'需要保留的正文');
 await userEvent.click(screen.getByRole('button',{name:'保存 Prompt'}));
 await waitFor(()=>expect(save).toHaveBeenCalledOnce());
 // The dialog only closes against a usable list, so the saved record must be
 // present with its content rather than a raw transport payload.
 await waitFor(()=>expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
 expect(current!.prompts.map(item=>item.id)).toContain('saved');
 expect(current!.prompts.find(item=>item.id==='saved')?.title).toBe('新条目');
});

it('previews full multiline text without copying and preserves purpose when editing',async()=>{
 const body='  第一行\n\n'+('很长的正文。'.repeat(120))+'\n最后一行  ';
 const item={...prompt,body,purpose:'整理采访记录'};
 const copy=vi.spyOn(api,'copyText').mockResolvedValue();
 const save=vi.spyOn(api,'savePrompt').mockImplementation(async (...[p]) =>p);
 render(<PromptsPage prompts={[item]} notify={vi.fn()} onSnapshot={vi.fn()}/>);
 await userEvent.click(screen.getByRole('button',{name:'查看 A first'}));
 expect(copy).not.toHaveBeenCalled();
 expect(screen.getByRole('dialog').querySelector('.prompt-preview-body')?.textContent).toBe(body);
 expect(screen.getByRole('dialog')).toHaveTextContent('整理采访记录');
 await userEvent.click(screen.getByRole('button',{name:'复制正文'}));
 expect(copy).toHaveBeenCalledWith(body);
 await userEvent.click(screen.getAllByRole('button',{name:'关闭'})[0]);
 await userEvent.click(screen.getByRole('button',{name:'编辑 Prompt'}));
 expect(screen.getByLabelText('用途（选填）')).toHaveValue(item.purpose);
 await userEvent.clear(screen.getByLabelText('用途（选填）'));
 await userEvent.type(screen.getByLabelText('用途（选填）'),'给访谈分类');
 await userEvent.click(screen.getByRole('button',{name:'保存 Prompt'}));
 await waitFor(()=>expect(save).toHaveBeenCalledWith(expect.objectContaining({body,purpose:'给访谈分类'})));
});
