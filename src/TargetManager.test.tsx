import { expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import * as api from './api';
import { TargetManager } from './TargetManager';
it('edits a known Agent install location while keeping identity fixed',async()=>{
 const target={id:'hermes',name:'Hermes',globalPath:'C:/fixture/hermes/skills',projectPath:'',enabled:true,available:false};
 const dispatch=vi.spyOn(api,'dispatch').mockImplementation(async()=>({targets:[target]}) as never);
 const notify=vi.fn();render(<TargetManager notify={notify}/>);
 expect(await screen.findByText('Hermes')).toBeVisible();
 expect(screen.queryByRole('button',{name:'添加 Agent'})).not.toBeInTheDocument();
 await userEvent.click(screen.getByRole('button',{name:'编辑位置'}));
 const field=screen.getByLabelText('所有项目使用的 Skill 位置');await userEvent.clear(field);await userEvent.type(field,'C:/fixture/new/skills');
 await userEvent.click(screen.getByRole('button',{name:'保存 Agent'}));
 await waitFor(()=>expect(notify).toHaveBeenCalledWith('Skill 安装位置已保存'));
 expect(dispatch).toHaveBeenCalledWith('targets.save',{target:{...target,globalPath:'C:/fixture/new/skills'}});
 vi.restoreAllMocks();
});
