export interface Settings {
  schemaVersion: number; apiKey: string; targetLanguage: string;
  theme: 'default' | 'amoled'; accentColor: string; interfaceLanguage: 'uk' | 'en';
  originalVolume: number; translationVolume: number; selectedSourceId: string | null;
  captureShortcut: string | null; shortcutMode: 'activeApplication' | 'lastCaptured'; lastCapturedSourceId: string | null;
}
export interface AudioSource { id: string; name: string; active: boolean; detail: string | null }
export interface Language { code: string; name: string }
export type Phase = 'idle' | 'starting' | 'running' | 'stopping' | 'error';
export interface DiagnosticSample { elapsedSeconds: number; sendingQueueSeconds: number; playbackQueueSeconds: number }
export interface NetworkSample { elapsedSeconds: number; rttMs: number }
export type DiagnosticIssue = 'networkUnstable' | 'sendingQueue' | 'playbackQueue';
export interface DiagnosticsSnapshot {
  elapsedSeconds: number; samples: DiagnosticSample[]; networkSamples: NetworkSample[];
  networkRttMs: number | null; sendDurationMs: number | null;
  sendingQueueSeconds: number; playbackQueueSeconds: number; issues: DiagnosticIssue[];
}
export const emptyDiagnostics = (): DiagnosticsSnapshot => ({ elapsedSeconds: 0, samples: [], networkSamples: [], networkRttMs: null, sendDurationMs: null, sendingQueueSeconds: 0, playbackQueueSeconds: 0, issues: [] });
export interface SessionStatus {
  sessionId: number; phase: Phase; elapsedSeconds: number; inputLevel: number;
  sentAudioSeconds: number; receivedAudioSeconds: number; playbackBacklogSeconds: number; playbackSpeed: number; message: string | null;
  diagnostics: DiagnosticsSnapshot;
}
export interface ConnectionReport { model: string; elapsedMs: number }
export const defaults = (): Settings => ({ schemaVersion: 1, apiKey: '', targetLanguage: 'uk', theme: 'default', accentColor: '#c5e89d', interfaceLanguage: 'uk', originalVolume: .15, translationVolume: .7, selectedSourceId: null, captureShortcut: null, shortcutMode: 'activeApplication', lastCapturedSourceId: null });
export const idle = (): SessionStatus => ({ sessionId: 0, phase: 'idle', elapsedSeconds: 0, inputLevel: 0, sentAudioSeconds: 0, receivedAudioSeconds: 0, playbackBacklogSeconds: 0, playbackSpeed: 1, message: null, diagnostics: emptyDiagnostics() });
