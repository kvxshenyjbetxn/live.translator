export interface Settings {
  schemaVersion: number; apiKey: string; targetLanguage: string;
  originalVolume: number; translationVolume: number; selectedSourceId: string | null;
}
export interface AudioSource { id: string; name: string; active: boolean; detail: string | null }
export interface Language { code: string; name: string }
export type Phase = 'idle' | 'starting' | 'running' | 'stopping' | 'error';
export interface SessionStatus {
  sessionId: number; phase: Phase; elapsedSeconds: number; inputLevel: number;
  sentAudioSeconds: number; receivedAudioSeconds: number; playbackBacklogSeconds: number; playbackSpeed: number; message: string | null;
}
export interface ConnectionReport { model: string; elapsedMs: number }
export const defaults = (): Settings => ({ schemaVersion: 1, apiKey: '', targetLanguage: 'uk', originalVolume: .15, translationVolume: .7, selectedSourceId: null });
export const idle = (): SessionStatus => ({ sessionId: 0, phase: 'idle', elapsedSeconds: 0, inputLevel: 0, sentAudioSeconds: 0, receivedAudioSeconds: 0, playbackBacklogSeconds: 0, playbackSpeed: 1, message: null });
