import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, fireEvent, waitFor } from '@testing-library/svelte';

// Mock the Tauri dialog plugin and convertFileSrc before importing the
// component — the picker is invoked inside MessageInput's setup code.
vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn(),
}));
vi.mock('@tauri-apps/api/core', () => ({
  // Tests don't render the actual file; just hand back something that
  // doesn't throw inside <img src=...>.
  convertFileSrc: (path: string) => `mock-asset://${path}`,
}));

import { open } from '@tauri-apps/plugin-dialog';
import MessageInput from './MessageInput.svelte';

beforeEach(() => {
  vi.mocked(open).mockReset();
});

describe('MessageInput', () => {
  it('renders a textarea, send button, and attach button', () => {
    const onSend = vi.fn();
    const { getByLabelText, getByRole, getByTestId } = render(MessageInput, {
      props: { onSend },
    });
    expect(getByLabelText('Message')).toBeTruthy();
    expect(getByRole('button', { name: /send/i })).toBeTruthy();
    expect(getByTestId('attach-button')).toBeTruthy();
  });

  it('calls onSend with input text and empty attachments on click', async () => {
    const onSend = vi.fn();
    const { getByLabelText, getByRole } = render(MessageInput, {
      props: { onSend },
    });
    const ta = getByLabelText('Message') as HTMLTextAreaElement;
    await fireEvent.input(ta, { target: { value: 'Hello!' } });
    await fireEvent.click(getByRole('button', { name: /send/i }));
    expect(onSend).toHaveBeenCalledWith('Hello!', []);
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

  it('does not call onSend on empty input with no attachments', async () => {
    const onSend = vi.fn();
    const { getByRole } = render(MessageInput, { props: { onSend } });
    await fireEvent.click(getByRole('button', { name: /send/i }));
    expect(onSend).not.toHaveBeenCalled();
  });

  it('cmd+enter / ctrl+enter submits with attachments forwarded', async () => {
    const onSend = vi.fn();
    const { getByLabelText } = render(MessageInput, { props: { onSend } });
    const ta = getByLabelText('Message') as HTMLTextAreaElement;
    await fireEvent.input(ta, { target: { value: 'shortcut' } });
    await fireEvent.keyDown(ta, { key: 'Enter', ctrlKey: true });
    expect(onSend).toHaveBeenCalledWith('shortcut', []);
  });

  it('disables send and attach when disabled prop is true', () => {
    const onSend = vi.fn();
    const { getByRole, getByTestId } = render(MessageInput, {
      props: { onSend, disabled: true },
    });
    const send = getByRole('button', { name: /send/i }) as HTMLButtonElement;
    const attach = getByTestId('attach-button') as HTMLButtonElement;
    expect(send.disabled).toBe(true);
    expect(attach.disabled).toBe(true);
  });

  describe('attachments', () => {
    it('opens the file picker with image filter when 📎 is clicked', async () => {
      vi.mocked(open).mockResolvedValue(null);
      const onSend = vi.fn();
      const { getByTestId } = render(MessageInput, { props: { onSend } });
      await fireEvent.click(getByTestId('attach-button'));
      await waitFor(() => {
        expect(open).toHaveBeenCalledWith(
          expect.objectContaining({
            filters: [
              expect.objectContaining({
                name: 'Image',
                extensions: expect.arrayContaining(['png', 'jpg', 'jpeg', 'webp']),
              }),
            ],
          })
        );
      });
    });

    it('renders a chip with the filename and a remove button after a successful pick', async () => {
      vi.mocked(open).mockResolvedValue('/home/user/Pictures/design.png');
      const onSend = vi.fn();
      const { getByTestId, findAllByTestId, findByLabelText } = render(MessageInput, {
        props: { onSend },
      });
      await fireEvent.click(getByTestId('attach-button'));
      const chips = await findAllByTestId('attachment-chip');
      expect(chips).toHaveLength(1);
      expect(chips[0].textContent).toContain('design.png');
      // Remove button is reachable.
      expect(await findByLabelText('Remove attachment')).toBeTruthy();
    });

    it('skips files whose extension is not a supported image type', async () => {
      vi.mocked(open).mockResolvedValue('/home/user/notes.txt');
      const onSend = vi.fn();
      const { getByTestId, queryByTestId } = render(MessageInput, { props: { onSend } });
      await fireEvent.click(getByTestId('attach-button'));
      // Give the async picker time to resolve.
      await new Promise((r) => setTimeout(r, 10));
      expect(queryByTestId('attachment-chip')).toBeNull();
    });

    it('removes a chip when × is clicked', async () => {
      vi.mocked(open).mockResolvedValue('/home/user/a.png');
      const onSend = vi.fn();
      const { getByTestId, findByLabelText, queryByTestId } = render(MessageInput, {
        props: { onSend },
      });
      await fireEvent.click(getByTestId('attach-button'));
      const removeBtn = await findByLabelText('Remove attachment');
      await fireEvent.click(removeBtn);
      await waitFor(() => {
        expect(queryByTestId('attachment-chip')).toBeNull();
      });
    });

    it('forwards attachments to onSend with the inferred media_type', async () => {
      vi.mocked(open).mockResolvedValue('/home/user/screenshot.PNG');
      const onSend = vi.fn();
      const { getByTestId, getByLabelText, getByRole } = render(MessageInput, {
        props: { onSend },
      });
      await fireEvent.click(getByTestId('attach-button'));
      // Give the picker a tick to populate state.
      await waitFor(() => {
        expect(getByTestId('attachment-chip')).toBeTruthy();
      });
      const ta = getByLabelText('Message') as HTMLTextAreaElement;
      await fireEvent.input(ta, { target: { value: 'check this' } });
      await fireEvent.click(getByRole('button', { name: /send/i }));
      expect(onSend).toHaveBeenCalledWith('check this', [
        {
          sourcePath: '/home/user/screenshot.PNG',
          mediaType: 'image/png',
          filename: 'screenshot.PNG',
        },
      ]);
    });

    it('clears attachments after send', async () => {
      vi.mocked(open).mockResolvedValue('/home/user/a.png');
      const onSend = vi.fn();
      const { getByTestId, getByLabelText, getByRole, queryByTestId } = render(MessageInput, {
        props: { onSend },
      });
      await fireEvent.click(getByTestId('attach-button'));
      await waitFor(() => expect(getByTestId('attachment-chip')).toBeTruthy());
      const ta = getByLabelText('Message') as HTMLTextAreaElement;
      await fireEvent.input(ta, { target: { value: 'x' } });
      await fireEvent.click(getByRole('button', { name: /send/i }));
      await waitFor(() => expect(queryByTestId('attachment-chip')).toBeNull());
    });

    it('allows sending with attachments only (no text)', async () => {
      vi.mocked(open).mockResolvedValue('/home/user/a.png');
      const onSend = vi.fn();
      const { getByTestId, getByRole } = render(MessageInput, { props: { onSend } });
      await fireEvent.click(getByTestId('attach-button'));
      await waitFor(() => expect(getByTestId('attachment-chip')).toBeTruthy());
      const send = getByRole('button', { name: /send/i }) as HTMLButtonElement;
      expect(send.disabled).toBe(false);
      await fireEvent.click(send);
      expect(onSend).toHaveBeenCalledWith('', expect.any(Array));
      expect((onSend.mock.calls[0][1] as Array<unknown>).length).toBe(1);
    });

    it('cancelling the picker (null result) leaves attachments empty', async () => {
      vi.mocked(open).mockResolvedValue(null);
      const onSend = vi.fn();
      const { getByTestId, queryByTestId } = render(MessageInput, { props: { onSend } });
      await fireEvent.click(getByTestId('attach-button'));
      await new Promise((r) => setTimeout(r, 5));
      expect(queryByTestId('attachment-chip')).toBeNull();
    });
  });
});
