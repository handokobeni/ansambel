import { describe, it, expect, beforeEach } from 'vitest';
import { terminalSnapshots } from './terminal-snapshots';

describe('terminalSnapshots', () => {
  beforeEach(() => terminalSnapshots.reset());

  it('take returns the stored snapshot then clears it (one-shot)', () => {
    terminalSnapshots.set('term_a', 'SCREEN');
    expect(terminalSnapshots.take('term_a')).toBe('SCREEN');
    expect(terminalSnapshots.take('term_a')).toBeUndefined();
  });

  it('take returns undefined when nothing is stored', () => {
    expect(terminalSnapshots.take('missing')).toBeUndefined();
  });

  it('keeps snapshots isolated per terminal id', () => {
    terminalSnapshots.set('term_a', 'A');
    terminalSnapshots.set('term_b', 'B');
    expect(terminalSnapshots.take('term_b')).toBe('B');
    expect(terminalSnapshots.take('term_a')).toBe('A');
  });

  it('drop removes a snapshot without reading it', () => {
    terminalSnapshots.set('term_a', 'A');
    terminalSnapshots.drop('term_a');
    expect(terminalSnapshots.take('term_a')).toBeUndefined();
  });
});
