<script lang="ts">
  import { onMount } from 'svelte';
  import { version } from '../package.json';
  import { listen } from '@tauri-apps/api/event';
  import { CloseCoordinator } from './lib/close';
  import { api, desktop } from './bridge';
  import { defaults, idle, type Settings, type AudioSource, type Language } from './types';
  import { AutosaveStore, type SaveState } from './lib/autosave';
  import { newerStatus } from './lib/session-store';
  import { decideDesktop, MenuSync, type DesktopAction, type MenuModel, type ShortcutStatus } from './lib/desktop';
  import Sources from './components/Sources.svelte';
  import Translation from './components/Translation.svelte';
  import SettingsView from './components/Settings.svelte';
  let settings = $state(defaults());
  let status = $state(idle());
  let sources = $state<AudioSource[]>([]);
  let languages = $state<Language[]>([{ code: 'uk', name: 'Українська' }]);
  let directory = $state(''); let tab = $state<'translation' | 'settings'>('translation');
  let loaded = $state(false); let loading = $state(false); let closing = $state(false);
  let closeFailed = $state(false); let error = $state(''); let sourceError = $state('');
  let saveState = $state<SaveState>('loading');
  let checking = $state(false); let connectionMessage = $state(''); let connectionError = $state(false);
  let busy = $derived(['starting', 'running', 'stopping'].includes(status.phase));
  let selected = $derived(sources.find(source => source.id === settings.selectedSourceId));
  let shortcutStatus = $state<ShortcutStatus>({ active: null, error: null, integrationError: null });
  let recording = $state(false);
  let shortcutWarning = '';
  async function updateShortcutStatus() {
    try { const value = await api.shortcutStatus(); shortcutStatus = { ...value, error: value.error ?? (shortcutWarning || null) }; }
    catch { shortcutStatus.error = 'Не вдалося отримати стан глобальної клавіші.'; }
  }
  const autosave = new AutosaveStore(async value => {
    try {
      const outcome = await api.save(value);
      if (outcome.shortcutError) shortcutWarning = outcome.shortcutError;
      if (outcome.captureShortcut !== value.captureShortcut && settings.captureShortcut === value.captureShortcut) {
        settings = { ...settings, captureShortcut: outcome.captureShortcut };
        autosave.update(settings);
      }
    } finally { await updateShortcutStatus(); }
  }, state => { saveState = state; });
  const menuSync = new MenuSync<MenuModel>(api.syncDesktop);
  const liveVolumes = new AutosaveStore(async value => { await api.volumes(value.originalVolume, value.translationVolume); }, () => {}, 0);
  let fetchingSources = false; let polling = false; let starting = $state(false);
  let startEpoch = 0;
  let menuRevision = $state(0);
  let captured: { id: number; source: string } | null = null;
  let displayStatus = $derived(starting && !busy ? { ...status, phase: 'starting' as const } : status);
  $effect(() => {
    if (!desktop || !loaded) return;
    const allowed = !closing && !checking && !recording;
    const model: MenuModel = {
      revision: menuRevision,
      sources: sources.map(({ id, name }) => ({ id, name })), selectedSourceId: settings.selectedSourceId,
      targetLanguage: settings.targetLanguage, canSelect: allowed && !busy && !starting,
      canStart: allowed && !busy && !starting && !!selected && !!settings.apiKey.trim() && !sourceError,
      canStop: allowed && (busy || starting) && status.phase !== 'stopping',
    };
    void menuSync.update(model).catch(() => { error = 'Не вдалося оновити меню трея.'; });
  });
  async function refreshSources() {
    if (!desktop || fetchingSources || busy || closing) return;
    fetchingSources = true;
    try { sources = await api.sources(); sourceError = ''; }
    catch (reason) { sourceError = String(reason); }
    finally { fetchingSources = false; }
  }
  async function load() {
    loading = true; loaded = false; error = '';
    if (!desktop) { loading = false; return; }
    try {
      const [value, path, available] = await Promise.all([api.load(), api.directory(), api.languages()]);
      settings = value; directory = path; languages = available;
      autosave.init(value); liveVolumes.init(value); loaded = true;
      await updateShortcutStatus();
      await refreshSources();
    } catch (reason) { error = String(reason); }
    finally { loading = false; }
  }
  function change(patch: Partial<Settings>) {
    if (!loaded || closing || checking || ((patch.selectedSourceId !== undefined || patch.targetLanguage !== undefined) && (busy || starting))) return;
    if (patch.captureShortcut !== undefined) shortcutWarning = '';
    settings = { ...settings, ...patch };
    if (patch.apiKey !== undefined || patch.targetLanguage !== undefined) { connectionMessage = ''; connectionError = false; }
    autosave.update(settings);
    if (patch.originalVolume !== undefined || patch.translationVolume !== undefined) {
      liveVolumes.update(settings);
      void liveVolumes.flush().catch(() => { error = 'Не вдалося застосувати гучність. Зупиніть переклад і спробуйте ще раз.'; });
    }
  }
  async function poll() {
    if (!desktop || polling || !loaded || closing) return;
    polling = true;
    try {
      status = newerStatus(status, await api.status());
      if (captured?.id === status.sessionId && status.phase === 'running' && settings.lastCapturedSourceId !== captured.source) {
        change({ lastCapturedSourceId: captured.source });
      }
    }
    catch { error = 'Не вдалося отримати стан аудіосесії.'; }
    finally { polling = false; }
  }
  async function start(sourceId = settings.selectedSourceId) {
    if (!loaded || busy || starting || checking || recording || !sourceId || closing) return;
    starting = true; error = '';
    const epoch = ++startEpoch;
    try {
      // Re-discover the exact requested identity; never substitute another source.
      const available = await api.sources();
      if (epoch !== startEpoch || closing) return;
      sources = available; sourceError = '';
      if (!available.some(source => source.id === sourceId)) throw new Error('Програма недоступна для захоплення. Увімкніть у ній звук і спробуйте ще раз.');
      settings = { ...settings, selectedSourceId: sourceId };
      autosave.update(settings);
      await autosave.flush();
      if (epoch !== startEpoch || closing) return;
      const id = await api.start(sourceId);
      captured = { id, source: sourceId };
      status = newerStatus(status, { ...idle(), sessionId: id, phase: 'starting' });
      if (epoch !== startEpoch || closing) await api.stop();
      await poll();
    } catch (reason) { error = String(reason); }
    finally { starting = false; }
  }
  async function stop() {
    startEpoch++; error = '';
    try { await api.stop(); await poll(); }
    catch (reason) { error = String(reason); }
  }
  async function recordShortcut(value: boolean) {
    if (value && (closing || checking || starting || busy)) throw new Error('Спочатку зупиніть переклад.');
    recording = value;
    try { await api.suspendShortcut(value); }
    catch (reason) { recording = !value; throw reason; }
  }
  async function desktopAction(action: DesktopAction) {
    // Native check items auto-toggle even when the selected value is unchanged.
    // Reconcile after a menu click, not on every session-meter update.
    if (action.kind === 'source' || action.kind === 'language') menuRevision++;
    const decision = decideDesktop(action, { loaded, closing, checking, recording, starting, phase: status.phase, settings });
    switch (decision.kind) {
      case 'ignore': return;
      case 'stop': await stop(); break;
      case 'start': await start(decision.sourceId); break;
      case 'error': error = decision.message; break;
      case 'source': if (sources.some(source => source.id === decision.value)) change({ selectedSourceId: decision.value }); break;
      case 'language': if (languages.some(language => language.code === decision.value)) change({ targetLanguage: decision.value }); break;
      case 'refresh': await refreshSources(); break;
    }
    if (error) await api.show().catch(() => {});
  }
  async function checkConnection() {
    if (!loaded || busy || starting || checking || closing || !settings.apiKey.trim()) return;
    checking = true; connectionMessage = ''; connectionError = false;
    try {
      await autosave.flush();
      const report = await api.checkConnection();
      connectionMessage = `Google Live Translation доступний (${(report.elapsedMs / 1000).toFixed(2)} с). Сесію закрито, аудіо не передавалося. Це не гарантія тривалої роботи чи обсягу квоти.`;
    } catch (reason) { connectionError = true; connectionMessage = String(reason); }
    finally { checking = false; }
  }
  async function retrySave() { try { await autosave.retry(); error = ''; } catch (reason) { error = String(reason); } }
  async function openDirectory() { try { await api.openDirectory(); } catch (reason) { error = String(reason); } }
  const closeCoordinator = new CloseCoordinator({ begin: api.beginClose, flush: () => autosave.flush(), stop: api.stop, finish: api.finishClose, cancel: api.cancelClose });
  async function requestClose(discard = false) {
    if (closing && !closeFailed) return;
    closing = true; closeFailed = false;
    try { await (discard ? closeCoordinator.discard() : closeCoordinator.request()); }
    catch { closeFailed = true; }
  }
  function focusCloseDialog(node: HTMLDialogElement) { node.focus(); }
  async function cancelClose() {
    try { await closeCoordinator.cancel(); closing = false; closeFailed = false; await poll(); }
    catch { closeFailed = true; }
  }
  onMount(() => {
    let disposed = false;
    let releases: (() => void)[] = [];
    if (desktop) {
      void Promise.all([
        listen('app-close-requested', () => { void requestClose(); }),
        listen<DesktopAction>('desktop-action', event => { void desktopAction(event.payload); }),
      ]).then(async listeners => {
        if (disposed) { listeners.forEach(release => release()); return; }
        releases = listeners;
        await api.ready();
        await load();
      }).catch(() => { error = 'Не вдалося налаштувати desktop-керування. Перезапустіть програму.'; });
    }
    const statusTimer = setInterval(() => { void poll(); }, 150);
    const sourcesTimer = setInterval(() => { void refreshSources(); }, 3000);
    return () => { disposed = true; releases.forEach(release => release()); clearInterval(statusTimer); clearInterval(sourcesTimer); };
  });
</script>
<div class="shell" inert={closing}>
  <aside class="sidebar"><div class="brand"><span class="brand-mark">≋</span><div>Live Translator<small>ТВІЙ ЖИВИЙ ПЕРЕКЛАД</small></div></div>
    <button class="navigation-item" class:chosen={tab === 'translation'} onclick={() => tab = 'translation'}>◉ &nbsp; Переклад</button>
    <Sources {sources} selected={settings.selectedSourceId} disabled={!loaded || busy || starting || checking || closing} onSelect={id => { change({ selectedSourceId: id }); tab = 'translation'; }} onRefresh={() => { void refreshSources(); }}/>
    <div class="sidebar-bottom"><p>Захоплюється лише вибрана програма. Інші звуки не змінюються.</p><button class="navigation-item" class:chosen={tab === 'settings'} onclick={() => tab = 'settings'}>⚙ &nbsp; Налаштування</button><span class="version">v{version} · macOS-first</span></div>
  </aside>
  <div class="main-column"><main>
    {#if !desktop}<div class="notice">Це браузерний перегляд інтерфейсу. Захоплення звуку та збереження доступні лише в desktop-програмі.</div>{/if}
    {#if error}<div class="error" role="alert">{error}</div>{/if}
    {#if sourceError && tab === 'translation'}<div class="notice" role="alert">{sourceError}</div>{/if}
    {#if desktop && !loaded}<div class="notice">{loading ? 'Завантажуємо налаштування…' : 'Налаштування не завантажено. Файл не буде перезаписано.'}{#if !loading}<button class="secondary" onclick={() => { void load(); }}>Повторити завантаження</button>{/if}</div>{/if}
    {#if tab === 'settings'}<SettingsView {settings} {directory} state={saveState} disabled={!loaded || closing} {checking} connectionBlocked={busy || starting} {connectionMessage} {connectionError} {shortcutStatus} onRecording={recordShortcut} onCheck={() => { void checkConnection(); }} onChange={change} onRetry={() => { void retrySave(); }} onOpen={() => { void openDirectory(); }}/>
    {:else}<Translation {settings} status={displayStatus} sourceName={selected?.name ?? null} {languages} disabled={!loaded || closing || checking || recording} canStart={!!selected && !!settings.apiKey.trim() && !sourceError} onStart={() => { void start(); }} onStop={() => { void stop(); }} onChange={change}/>{/if}
  </main><footer><span class:error-text={saveState === 'error'}>{!desktop ? 'Лише попередній перегляд' : !loaded ? 'Налаштування недоступні' : saveState === 'error' ? '● Не збережено · відкрийте налаштування' : saveState === 'saving' || saveState === 'unsaved' ? '◌ Зберігаємо…' : '● Усі зміни збережено'}</span><span>Аудіозапис не зберігається</span></footer></div>
</div>
{#if closing}<div class="close-overlay"><dialog open class="close-dialog" aria-modal="true" aria-label="Завершення роботи" tabindex="-1" use:focusCloseDialog><h2>{closeFailed ? 'Не вдалося завершити роботу' : 'Завершуємо роботу…'}</h2><p>{closeFailed ? 'Не вдалося зберегти зміни або завершити очищення аудіо. Можна повторити спробу чи вийти без збереження; очищення аудіо все одно обов’язкове.' : 'Зберігаємо останні зміни та повертаємо оригінальний звук.'}</p>{#if closeFailed}<div class="close-actions"><button onclick={() => { void requestClose(); }}>Повторити</button><button onclick={() => { void requestClose(true); }}>Вийти без збереження</button><button onclick={() => { void cancelClose(); }}>Залишитися</button></div>{/if}</dialog></div>{/if}
