import { describe, it, expect, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import MessageInput from './MessageInput.svelte';

describe('MessageInput', () => {
  it('renders a textarea and a send button', () => {
    const onSend = vi.fn();
    const { getByLabelText, getByRole } = render(MessageInput, {
      props: { onSend },
    });
    expect(getByLabelText('Message')).toBeTruthy();
    expect(getByRole('button', { name: /send/i })).toBeTruthy();
  });

  it('calls onSend with input text on click', async () => {
    const onSend = vi.fn();
    const { getByLabelText, getByRole } = render(MessageInput, {
      props: { onSend },
    });
    const ta = getByLabelText('Message') as HTMLTextAreaElement;
    await fireEvent.input(ta, { target: { value: 'Hello!' } });
    await fireEvent.click(getByRole('button', { name: /send/i }));
    expect(onSend).toHaveBeenCalledWith('Hello!');
  });

  it('clears input after send', async () => {
    const onSend = vi.fn();
    const { getByLabelText, getByRole } = render(MessageInput, {
      props: { onSend },
    });
    const ta = getByLabelText('Message') as HTMLTextAreaElement;
    await fireEvent.input(ta, { target: { value: 'msg' } });
    await fireEvent.click(getByRole('button', { name: /send/i }));
    expect(ta.value).toBe('');
  });

  it('does not call onSend on empty input', async () => {
    const onSend = vi.fn();
    const { getByRole } = render(MessageInput, { props: { onSend } });
    await fireEvent.click(getByRole('button', { name: /send/i }));
    expect(onSend).not.toHaveBeenCalled();
  });

  it('cmd+enter / ctrl+enter submits', async () => {
    const onSend = vi.fn();
    const { getByLabelText } = render(MessageInput, { props: { onSend } });
    const ta = getByLabelText('Message') as HTMLTextAreaElement;
    await fireEvent.input(ta, { target: { value: 'shortcut' } });
    await fireEvent.keyDown(ta, { key: 'Enter', ctrlKey: true });
    expect(onSend).toHaveBeenCalledWith('shortcut');
  });

  it('cmd+enter (metaKey) submits', async () => {
    const onSend = vi.fn();
    const { getByLabelText } = render(MessageInput, { props: { onSend } });
    const ta = getByLabelText('Message') as HTMLTextAreaElement;
    await fireEvent.input(ta, { target: { value: 'mac shortcut' } });
    await fireEvent.keyDown(ta, { key: 'Enter', metaKey: true });
    expect(onSend).toHaveBeenCalledWith('mac shortcut');
  });

  it('disables send button when disabled prop is true', () => {
    const onSend = vi.fn();
    const { getByRole } = render(MessageInput, {
      props: { onSend, disabled: true },
    });
    const btn = getByRole('button', { name: /send/i }) as HTMLButtonElement;
    expect(btn.disabled).toBe(true);
  });
});
