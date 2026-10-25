import { expect, test } from 'vitest';
import { CloseCoordinator } from '../src/lib/close';
test('close gates new starts, flushes latest settings and waits for cleanup before exit', async () => {
  const order: string[] = [];
  const close = new CloseCoordinator({ begin: async () => { order.push('gate'); }, flush: async () => { order.push('save'); }, stop: async () => { order.push('stop'); }, finish: async () => { order.push('exit'); }, cancel: async () => {} });
  await close.request(); expect(order).toEqual(['gate', 'save', 'stop', 'exit']);
});
test('failed save blocks exit, explicit discard still waits for audio cleanup', async () => {
  const order: string[] = []; let fail = true;
  const close = new CloseCoordinator({ begin: async () => {}, flush: async () => { if (fail) throw new Error('disk'); }, stop: async () => { order.push('stop'); }, finish: async () => { order.push('exit'); }, cancel: async () => {} });
  await expect(close.request()).rejects.toThrow(); expect(order).toEqual([]);
  fail = false; await close.discard(); expect(order).toEqual(['stop', 'exit']);
});
test('cancel close reopens UI and does not run finish', async () => {
  let cancelled = false; let finished = false;
  const close = new CloseCoordinator({ begin: async () => {}, flush: async () => { throw new Error('disk'); }, stop: async () => {}, finish: async () => { finished = true; }, cancel: async () => { cancelled = true; } });
  await expect(close.request()).rejects.toThrow(); await close.cancel();
  expect(cancelled).toBe(true); expect(finished).toBe(false);
});
