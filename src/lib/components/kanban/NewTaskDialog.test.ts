// src/lib/components/kanban/NewTaskDialog.test.ts
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import NewTaskDialog from './NewTaskDialog.svelte';

describe('NewTaskDialog', () => {
  it('renders title input and description textarea', () => {
    render(NewTaskDialog, {
      props: { open: true, onSubmit: vi.fn(), onCancel: vi.fn() },
    });
    expect(screen.getByLabelText(/title/i)).toBeTruthy();
    expect(screen.getByLabelText(/description/i)).toBeTruthy();
  });

  it('submit button is disabled when title is empty', () => {
    render(NewTaskDialog, {
      props: { open: true, onSubmit: vi.fn(), onCancel: vi.fn() },
    });
    const submitBtn = screen.getByRole('button', { name: /add task/i });
    expect((submitBtn as HTMLButtonElement).disabled).toBe(true);
  });

  it('calls onSubmit with title and description when form submitted', async () => {
    const onSubmit = vi.fn();
    render(NewTaskDialog, {
      props: { open: true, onSubmit, onCancel: vi.fn() },
    });
    const titleInput = screen.getByLabelText(/title/i);
    const descInput = screen.getByLabelText(/description/i);
    await fireEvent.input(titleInput, { target: { value: 'New feature' } });
    await fireEvent.input(descInput, {
      target: { value: 'Add the new feature' },
    });
    await fireEvent.click(screen.getByRole('button', { name: /add task/i }));
    expect(onSubmit).toHaveBeenCalledWith({
      title: 'New feature',
      description: 'Add the new feature',
    });
  });

  it('calls onCancel when Cancel button clicked', async () => {
    const onCancel = vi.fn();
    render(NewTaskDialog, {
      props: { open: true, onSubmit: vi.fn(), onCancel },
    });
    await fireEvent.click(screen.getByRole('button', { name: /cancel/i }));
    expect(onCancel).toHaveBeenCalled();
  });

  it('does not call onSubmit when form submitted with empty title (canSubmit guard)', async () => {
    const onSubmit = vi.fn();
    render(NewTaskDialog, {
      props: { open: true, onSubmit, onCancel: vi.fn() },
    });
    // Submit form directly without filling in title — hits the !canSubmit return branch
    const form = document.querySelector('form') as HTMLFormElement;
    await fireEvent.submit(form);
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it('does not render dialog content when open is false', () => {
    render(NewTaskDialog, {
      props: { open: false, onSubmit: vi.fn(), onCancel: vi.fn() },
    });
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('calls onCancel when backdrop is clicked', async () => {
    const onCancel = vi.fn();
    render(NewTaskDialog, {
      props: { open: true, onSubmit: vi.fn(), onCancel },
    });
    const backdrop = document.querySelector('.dialog-backdrop') as HTMLElement;
    await fireEvent.click(backdrop);
    expect(onCancel).toHaveBeenCalled();
  });
});
