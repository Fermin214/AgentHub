import { StrictMode, useState } from 'react';
import { expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Modal, TabList, Toast } from './ui';

function DialogFixture() {
  const [open, setOpen] = useState(false);
  const [child, setChild] = useState(false);
  return <><button onClick={() => setOpen(true)}>打开编辑</button>{open && <Modal title="编辑" onClose={() => setOpen(false)} footer={<button onClick={() => setOpen(false)}>完成</button>}><input aria-label="内容" autoFocus/><button onClick={() => setChild(true)}>查看详情</button>{child && <Modal title="详情" onClose={() => setChild(false)}><button onClick={() => { setChild(false); setOpen(false); }}>全部关闭</button></Modal>}</Modal>}<Toast message="保存失败" tone="error" onClose={() => {}}/></>;
}

it('contains keyboard focus, keeps feedback readable, and returns to the trigger', async () => {
  const user = userEvent.setup();
  render(<StrictMode><DialogFixture/></StrictMode>);
  const trigger = screen.getByRole('button', { name: '打开编辑' });
  await user.click(trigger);
  expect(screen.getByRole('textbox', { name: '内容' })).toHaveFocus();
  expect(screen.queryByRole('button', { name: '打开编辑' })).not.toBeInTheDocument();
  expect(screen.getByRole('status')).toHaveTextContent('保存失败');
  await user.tab({ shift: true });
  const close = screen.getByRole('button', { name: '关闭' });
  expect(close).toHaveFocus();
  await user.tab({ shift: true });
  expect(screen.getByRole('button', { name: '完成' })).toHaveFocus();
  await user.tab();
  expect(close).toHaveFocus();
  await user.keyboard('{Escape}');
  expect(trigger).toHaveFocus();
  await user.click(trigger);
  expect(screen.getByRole('textbox', { name: '内容' })).toHaveFocus();
  await user.click(screen.getByRole('button', { name: '完成' }));
  expect(trigger).toHaveFocus();
});

it('closes only the top dialog and restores the page after nested dialogs unmount together', async () => {
  const user = userEvent.setup();
  render(<DialogFixture/>);
  const trigger = screen.getByRole('button', { name: '打开编辑' });
  await user.click(trigger);
  const details = screen.getByRole('button', { name: '查看详情' });
  await user.click(details);
  expect(screen.getAllByRole('dialog')).toHaveLength(1);
  expect(screen.getByRole('dialog')).toHaveAccessibleName('详情');
  await user.keyboard('{Escape}');
  expect(screen.getByRole('dialog')).toHaveAccessibleName('编辑');
  expect(details).toHaveFocus();
  await user.click(details);
  await user.click(screen.getByRole('button', { name: '全部关闭' }));
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  expect(trigger).toHaveFocus();
  expect(document.querySelector('[inert]')).toBeNull();
});

it('moves through tabs with arrow keys and leaves only the selected tab in the Tab order', async () => {
  function Fixture() {
    const [value, setValue] = useState('one');
    return <><TabList id="fixture" label="分组" value={value} items={[{ id: 'one', label: '第一组' }, { id: 'two', label: '第二组' }, { id: 'three', label: '第三组' }]} onChange={setValue}/><div role="tabpanel" id="fixture-panel" aria-labelledby={`fixture-tab-${value}`}><button>页面操作</button></div></>;
  }
  const user = userEvent.setup();
  render(<Fixture/>);
  await user.tab();
  await user.keyboard('{ArrowLeft}');
  expect(screen.getByRole('tab', { name: '第三组' })).toHaveFocus();
  expect(screen.getByRole('tabpanel')).toHaveAccessibleName('第三组');
  await user.keyboard('{Home}{ArrowRight}');
  expect(screen.getByRole('tab', { name: '第二组' })).toHaveAttribute('aria-selected', 'true');
  await user.tab();
  expect(screen.getByRole('button', { name: '页面操作' })).toHaveFocus();
});
