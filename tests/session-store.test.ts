import { expect, test } from 'vitest';
import { newerStatus } from '../src/lib/session-store';
import { idle } from '../src/types';
test('late events cannot replace a newer session', () => {
  const current = { ...idle(), sessionId: 2, phase: 'running' as const };
  expect(newerStatus(current, { ...idle(), sessionId: 1 })).toBe(current);
  expect(newerStatus(current, { ...idle(), sessionId: 3 })).toMatchObject({ sessionId: 3 });
});
