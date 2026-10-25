import type { SessionStatus } from '../types';
export function newerStatus(current: SessionStatus, incoming: SessionStatus): SessionStatus {
  return incoming.sessionId < current.sessionId ? current : incoming;
}
