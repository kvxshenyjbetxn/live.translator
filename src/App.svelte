<script lang="ts">
  import { onMount } from 'svelte';
  import { applyAppearance } from './lib/appearance';
  import { t, locale } from './lib/i18n';
  import { version } from '../package.json';
  import { listen } from '@tauri-apps/api/event';
  import { CloseCoordinator } from './lib/close';
  import { api, desktop } from './bridge';
  import { defaults, idle, type Settings, type AudioSource, type Language } from './types';
  import { AutosaveStore, type SaveState } from './lib/autosave';
  import { newerStatus } from './lib/session-store';
  import { sourceCatalog, toggleFavorite, toggleHidden, preferenceError, recordName, isFavorite, type SourceView } from './lib/sources';
  import { decideDesktop, MenuSync, type DesktopAction, type MenuModel, type ShortcutStatus } from './lib/desktop';
  import Sources from './components/Sources.svelte';
  import Translation from './components/Translation.svelte';
  import SettingsView from './components/Settings.svelte';
  let settings = $state(defaults());
  $effect(() => { applyAppearance(settings); });
  $effect(() => { locale.set(settings.interfaceLanguage); document.documentElement.lang = settings.interfaceLanguage; });
  let status = $state(idle());
  let sources = $state<AudioSource[]>([]);
  let languages = $state<Language[]>([{ code: 'uk', name: 'Українська' }]);
  let directory = $state(''); let tab = $state<'translation' | 'settings'>('translation');
  let loaded = $state(false); let loading = $state(false); let closing = $state(false);
  let closeFailed = $state(false); let error = $state(''); let sourceError = $state('');
  let saveState = $state<SaveState>('loading');
  let checking = $state(false); let connectionMessage = $state(''); let connectionError = $state(false);
  let busy = $derived(['starting', 'running', 'stopping'].includes(status.phase));
  let started = $state<{ id: string; name: string } | null>(null);
  // An explicit start of a favorite that is not listed right now. Not persisted: it exists only
  // until it is cancelled, satisfied, or the user starts something else.
  let waiting = $state<{ id: string; name: string } | null>(null);
  // The explicit start target captured before discovery; keeps the label when a preference edit
  // removes the row while the attempt is in flight.
  let startTarget = $state<{ id: string; name: string } | null>(null);
  let preferenceRefusal = $state('');
  const catalog = $derived(sourceCatalog(sources, settings.favoriteSources, settings.hiddenSources));
  const listed = $derived(catalog.all.find(source => source.id === settings.selectedSourceId));
  const canStartSource = $derived(!!listed && (listed.available || listed.favorite) && !!settings.apiKey.trim() && !sourceError);
  const selectedName = $derived(listed?.name ?? (waiting && waiting.id === settings.selectedSourceId ? waiting.name : null) ?? (startTarget && startTarget.id === settings.selectedSourceId ? startTarget.name : null) ?? (started && started.id === settings.selectedSourceId ? started.name : null) ?? (settings.selectedSourceId ? recordName(settings.favoriteSources, settings.hiddenSources, settings.selectedSourceId) : null));
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
      revision: menuRevision, interfaceLanguage: settings.interfaceLanguage,
      sources: catalog.visible.map(({ id, name }) => ({ id, name })), selectedSourceId: settings.selectedSourceId,
      targetLanguage: settings.targetLanguage, canSelect: allowed && !busy && !starting && !waiting,
      canStart: allowed && !busy && !starting && !waiting && canStartSource,
      canStop: allowed && (busy || starting || !!waiting) && status.phase !== 'stopping',
      waiting: !!waiting,
    };
    void menuSync.update(model).catch(() => { error = 'Не вдалося оновити меню трея.'; });
  });
  async function refreshSources() {
    if (!desktop || fetchingSources || busy || closing) return;
    fetchingSources = true;
    let pending: { id: string; name: string } | null = null;
    try {
      sources = await api.sources(); sourceError = '';
      pending = waiting;
    }
    catch (reason) { sourceError = String(reason); }
    finally { fetchingSources = false; }
    // Only the exact waited-for id starts, and only while that same wait is still alive. A stop or a
    // selection change during discovery replaced it, so the late listing must not start anything.
    if (pending && waiting === pending && sources.some(source => source.id === pending.id)) await start(pending.id, pending);
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
  function favorite(view: SourceView) {
    const favoriteSources = toggleFavorite(settings.favoriteSources, { id: view.id, name: view.name });
    changePreference({ favoriteSources }, preferenceError(settings, { favoriteSources }));
  }
  function hide(view: SourceView) {
    const hiddenSources = toggleHidden(settings.hiddenSources, { id: view.id, name: view.name });
    changePreference({ hiddenSources }, preferenceError(settings, { hiddenSources }));
  }
  function restore(view: SourceView) {
    const hiddenSources = settings.hiddenSources.filter(record => record.id !== view.id);
    changePreference({ hiddenSources }, preferenceError(settings, { hiddenSources }));
  }
  function changePreference(patch: Partial<Settings>, problem: string | null) {
    if (problem) { preferenceRefusal = problem; error = problem; return; }
    // The refusal banner belongs to preference edits only; a later unrelated error stays visible.
    if (preferenceRefusal && error === preferenceRefusal) error = '';
    preferenceRefusal = '';
    change(patch);
  }
  function change(patch: Partial<Settings>) {
    if (!loaded || closing || checking || ((patch.selectedSourceId !== undefined || patch.targetLanguage !== undefined) && (busy || starting))) return;
    if (patch.selectedSourceId !== undefined && patch.selectedSourceId !== settings.selectedSourceId) { startEpoch++; waiting = null; }
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
  async function start(sourceId = settings.selectedSourceId, continued?: { id: string; name: string }) {
    if (!loaded || busy || starting || checking || recording || !sourceId || closing) return;
    // Capture the requested id/name and waiting eligibility before the first await: star/hide
    // edits made during discovery must not rewrite what the user explicitly asked to capture.
    const intent = continued ?? (isFavorite(settings.favoriteSources, sourceId)
      ? { id: sourceId, name: recordName(settings.favoriteSources, [], sourceId) ?? sourceId }
      : null);
    startTarget = intent;
    starting = true; error = ''; waiting = null;
    const epoch = ++startEpoch;
    try {
      // Re-discover the exact requested identity; never substitute another source.
      let available: AudioSource[];
      try { available = await api.sources(); }
      catch (reason) {
        // A transient rediscovery failure must not consume an armed explicit wait. Only Stop, close,
        // selection change or a completed session end the wait; those bump the epoch or set closing,
        // and then the late failure belongs to a cancelled attempt and is not reported either.
        if (epoch === startEpoch && !closing) {
          if (continued) waiting = continued;
          error = String(reason);
        }
        return;
      }
      if (epoch !== startEpoch || closing) return;
      sources = available; sourceError = '';
      const found = available.find(source => source.id === sourceId);
      if (!found) {
        // Only the captured explicit intent can wait; current favorite membership is not re-read
        // after discovery, so unstarring during the await cannot cancel the request.
        if (!intent) throw new Error('Програма недоступна для захоплення. Увімкніть у ній звук і спробуйте ще раз.');
        waiting = intent;
        if (settings.selectedSourceId !== intent.id) { settings = { ...settings, selectedSourceId: intent.id }; autosave.update(settings); }
        return;
      }
      const name = found.name;
      startTarget = { id: sourceId, name };
      settings = { ...settings, selectedSourceId: sourceId };
      autosave.update(settings);
      await autosave.flush();
      if (epoch !== startEpoch || closing) return;
      const id = await api.start(sourceId);
      // Stopped or closed while the native start was in flight: close exactly that late session
      // instead of adopting it, and never adopt it into newer UI state.
      if (epoch !== startEpoch || closing) { await api.stop(); return; }
      started = { id: sourceId, name };
      captured = { id, source: sourceId };
      status = newerStatus(status, { ...idle(), sessionId: id, phase: 'starting' });
      await poll();
    } catch (reason) { error = String(reason); }
    finally { starting = false; }
  }
  async function stop() {
    startEpoch++; waiting = null; error = '';
    try { await api.stop(); await poll(); }
    catch (reason) { error = String(reason); }
  }
  async function recordShortcut(value: boolean) {
    if (value && (closing || checking || starting || busy || waiting)) throw new Error('Спочатку зупиніть переклад.');
    recording = value;
    try { await api.suspendShortcut(value); }
    catch (reason) { recording = !value; throw reason; }
  }
  async function desktopAction(action: DesktopAction) {
    // Native check items auto-toggle even when the selected value is unchanged.
    // Reconcile after a menu click, not on every session-meter update.
    if (action.kind === 'source' || action.kind === 'language') menuRevision++;
    const decision = decideDesktop(action, { loaded, closing, checking, recording, starting, waiting: !!waiting, phase: status.phase, settings });
    switch (decision.kind) {
      case 'ignore': return;
      case 'stop': await stop(); break;
      case 'start': await start(decision.sourceId); break;
      case 'error': error = decision.message; break;
      case 'source': if (catalog.visible.some(source => source.id === decision.value)) change({ selectedSourceId: decision.value }); break;
      case 'language': if (languages.some(language => language.code === decision.value)) change({ targetLanguage: decision.value }); break;
      case 'refresh': await refreshSources(); break;
    }
    if (error) await api.show().catch(() => {});
  }
  async function checkConnection() {
    if (!loaded || busy || starting || checking || closing || waiting || !settings.apiKey.trim()) return;
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
    startEpoch++; waiting = null;
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
  <aside class="sidebar"><div class="brand"><span class="brand-mark">≋</span><div>Live Translator<small>{$t('ТВІЙ ЖИВИЙ ПЕРЕКЛАД')}</small></div></div>
    <button class="navigation-item" class:chosen={tab === 'translation'} onclick={() => tab = 'translation'}>◉ &nbsp; {$t('Переклад')}</button>
    <Sources visible={catalog.visible} hidden={catalog.hidden} selectedId={settings.selectedSourceId} disabled={!loaded || busy || starting || checking || closing} controlsDisabled={!loaded || closing || checking} onSelect={id => { change({ selectedSourceId: id }); tab = 'translation'; }} onRefresh={() => { void refreshSources(); }} onToggleFavorite={favorite} onHide={hide} onRestore={restore}/>
    <div class="sidebar-bottom"><p>{$t('Захоплюється лише вибрана програма. Інші звуки не змінюються.')}</p><button class="navigation-item" class:chosen={tab === 'settings'} onclick={() => tab = 'settings'}>⚙ &nbsp; {$t('Налаштування')}</button><span class="version">v{version} · macOS-first</span></div>
  </aside>
  <div class="main-column"><main>
    {#if !desktop}<div class="notice">{$t('Це браузерний перегляд інтерфейсу. Захоплення звуку та збереження доступні лише в desktop-програмі.')}</div>{/if}
    {#if error}<div class="error" role="alert">{$t(error)}</div>{/if}
    {#if sourceError && tab === 'translation'}<div class="notice" role="alert">{$t(sourceError)}</div>{/if}
    {#if desktop && !loaded}<div class="notice">{$t(loading ? 'Завантажуємо налаштування…' : 'Налаштування не завантажено. Файл не буде перезаписано.')}{#if !loading}<button class="secondary" onclick={() => { void load(); }}>{$t('Повторити завантаження')}</button>{/if}</div>{/if}
    {#if tab === 'settings'}<SettingsView {settings} {directory} state={saveState} disabled={!loaded || closing} {checking} connectionBlocked={busy || starting || !!waiting} {connectionMessage} {connectionError} {shortcutStatus} onRecording={recordShortcut} onCheck={() => { void checkConnection(); }} onChange={change} onRetry={() => { void retrySave(); }} onOpen={() => { void openDirectory(); }}/>
    {:else}<Translation {settings} status={displayStatus} sourceName={selectedName} {languages} disabled={!loaded || closing || checking || recording} canStart={canStartSource} waiting={!!waiting} onStart={() => { void start(); }} onStop={() => { void stop(); }} onChange={change}/>{/if}
  </main><footer><span class:error-text={saveState === 'error'}>{$t(!desktop ? 'Лише попередній перегляд' : !loaded ? 'Налаштування недоступні' : saveState === 'error' ? '● Не збережено · відкрийте налаштування' : saveState === 'saving' || saveState === 'unsaved' ? '◌ Зберігаємо…' : '● Усі зміни збережено')}</span><span>{$t('Аудіозапис не зберігається')}</span></footer></div>
</div>
{#if closing}<div class="close-overlay"><dialog open class="close-dialog" aria-modal="true" aria-label={$t('Завершення роботи')} tabindex="-1" use:focusCloseDialog><h2>{$t(closeFailed ? 'Не вдалося завершити роботу' : 'Завершуємо роботу…')}</h2><p>{$t(closeFailed ? 'Не вдалося зберегти зміни або завершити очищення аудіо. Можна повторити спробу чи вийти без збереження; очищення аудіо все одно обов’язкове.' : 'Зберігаємо останні зміни та повертаємо оригінальний звук.')}</p>{#if closeFailed}<div class="close-actions"><button onclick={() => { void requestClose(); }}>{$t('Повторити')}</button><button onclick={() => { void requestClose(true); }}>{$t('Вийти без збереження')}</button><button onclick={() => { void cancelClose(); }}>{$t('Залишитися')}</button></div>{/if}</dialog></div>{/if}
