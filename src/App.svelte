<script lang="ts">
  import { onMount } from 'svelte';
  import { version } from '../package.json';
  import { listen } from '@tauri-apps/api/event';
  import { CloseCoordinator } from './lib/close';
  import { api, desktop } from './bridge';
  import { defaults, idle, type Settings, type AudioSource, type Language } from './types';
  import { AutosaveStore, type SaveState } from './lib/autosave';
  import { newerStatus } from './lib/session-store';
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
  const autosave = new AutosaveStore(api.save, state => { saveState = state; });
  const liveVolumes = new AutosaveStore(async value => { await api.volumes(value.originalVolume, value.translationVolume); }, () => {}, 0);
  let fetchingSources = false; let polling = false; let starting = $state(false);
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
      await refreshSources();
    } catch (reason) { error = String(reason); }
    finally { loading = false; }
  }
  function change(patch: Partial<Settings>) {
    if (!loaded || closing || checking || ((patch.selectedSourceId !== undefined || patch.targetLanguage !== undefined) && busy)) return;
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
    try { status = newerStatus(status, await api.status()); }
    catch { error = 'Не вдалося отримати стан аудіосесії.'; }
    finally { polling = false; }
  }
  async function start() {
    if (!loaded || busy || starting || checking || !selected || closing) return;
    starting = true; error = '';
    try {
      await autosave.flush();
      const id = await api.start(selected.id);
      status = { ...idle(), sessionId: id, phase: 'starting' };
      await poll();
    } catch (reason) { error = String(reason); }
    finally { starting = false; }
  }
  async function stop() {
    error = '';
    try { await api.stop(); await poll(); }
    catch (reason) { error = String(reason); }
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
    let unlisten: (() => void) | undefined;
    if (desktop) {
      void listen('app-close-requested', () => { void requestClose(); }).then(async release => {
        if (disposed) { release(); return; }
        unlisten = release;
        await api.ready();
        await load();
      }).catch(() => { error = 'Не вдалося налаштувати завершення роботи. Перезапустіть програму.'; });
    }
    const statusTimer = setInterval(() => { void poll(); }, 150);
    const sourcesTimer = setInterval(() => { void refreshSources(); }, 3000);
    return () => { disposed = true; unlisten?.(); clearInterval(statusTimer); clearInterval(sourcesTimer); };
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
    {#if tab === 'settings'}<SettingsView {settings} {directory} state={saveState} disabled={!loaded || closing} {checking} connectionBlocked={busy || starting} {connectionMessage} {connectionError} onCheck={() => { void checkConnection(); }} onChange={change} onRetry={() => { void retrySave(); }} onOpen={() => { void openDirectory(); }}/>
    {:else}<Translation {settings} {status} sourceName={selected?.name ?? null} {languages} disabled={!loaded || closing || starting || checking} canStart={!!selected && !!settings.apiKey.trim() && !sourceError} onStart={() => { void start(); }} onStop={() => { void stop(); }} onChange={change}/>{/if}
  </main><footer><span class:error-text={saveState === 'error'}>{!desktop ? 'Лише попередній перегляд' : !loaded ? 'Налаштування недоступні' : saveState === 'error' ? '● Не збережено · відкрийте налаштування' : saveState === 'saving' || saveState === 'unsaved' ? '◌ Зберігаємо…' : '● Усі зміни збережено'}</span><span>Аудіозапис не зберігається</span></footer></div>
</div>
{#if closing}<div class="close-overlay"><dialog open class="close-dialog" aria-modal="true" aria-label="Завершення роботи" tabindex="-1" use:focusCloseDialog><h2>{closeFailed ? 'Не вдалося завершити роботу' : 'Завершуємо роботу…'}</h2><p>{closeFailed ? 'Не вдалося зберегти зміни або завершити очищення аудіо. Можна повторити спробу чи вийти без збереження; очищення аудіо все одно обов’язкове.' : 'Зберігаємо останні зміни та повертаємо оригінальний звук.'}</p>{#if closeFailed}<div class="close-actions"><button onclick={() => { void requestClose(); }}>Повторити</button><button onclick={() => { void requestClose(true); }}>Вийти без збереження</button><button onclick={() => { void cancelClose(); }}>Залишитися</button></div>{/if}</dialog></div>{/if}
