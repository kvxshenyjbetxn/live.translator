# Live Translator

**Завантаження для macOS Apple Silicon:** [GitHub Releases](https://github.com/kvxshenyjbetxn/live.translator/releases). Розпакуйте ZIP і відкрийте `Live Translator.app`. Програма ще не нотаризована Apple; macOS може попросити окремий дозвіл на запуск. Windows/Linux-аудіомодулі поки не готові.

macOS-first desktop application on **Rust + Tauri 2 + Svelte 5**. Select an application's audio, translate through the official Gemini Live Translation API, and listen with separate original/translation volume controls.

The main application lives at the project root. `crates/translator-core` is its shared Rust library, `src` is the UI, and `src-tauri` is the desktop shell. `spikes` contains earlier disposable experiments and is not used by the product at runtime.

## Current platform support

- **macOS 14.2+**: real application audio capture via Core Audio process taps; development/packaging tested on Apple Silicon, macOS 26.5.
- **Windows/Linux**: shared architecture and explicit unsupported capture backend. Real WASAPI/PipeWire implementations and testing on those OSes remain future work. Do not mistake a macOS build for verified cross-platform operation.

## Develop and build

Requirements: recent Rust stable (tested 1.95), Node 22.12+/24+/26+ (tested 26), npm, Xcode + Command Line Tools on Mac. No Python/Swift helper is required at runtime.

```sh
npm ci
npm run tauri -- dev

# Local macOS app bundle; no publishing, paid services or signing-account changes:
npm run tauri -- build --bundles app
# Faster local debug bundle:
npm run tauri -- build --debug --bundles app
```

Build output: `target/release/bundle/macos/Live Translator.app` (or `target/debug/bundle/macos/Live Translator.app` with `--debug`). No auto-update, background startup or automatic capture on launch.

## First use

1. Open Settings / «Налаштування». Paste your Google AI Studio API key. It is masked by default, with explicit reveal and delete actions. Use **«Перевірити з’єднання»** beside the key: settings are flushed first, then a short Live Translation session is opened and closed **without audio**. It checks access to the actual model, not only an HTTP endpoint. Stop translation first; the test cannot run concurrently with capture or another check. It does not establish remaining quota or guarantee a later uninterrupted stream.
2. Changes save automatically. Wait for the saved indicator; failures stay visible and can be retried. Closing flushes pending edits or offers explicit retry/discard.
3. Refresh/select the source in the sidebar, choose the target language and click «Почати переклад». Startup begins immediately, without an artificial countdown; Google connection setup still takes time and can be cancelled. If the source is not listed yet, briefly play audio and refresh first.
4. Grant macOS **System Audio Recording** permission if requested (System Settings → Privacy & Security; wording varies by OS release). No microphone access is required. After changing permissions, macOS may require restarting the app.
5. Adjust original and translated volume independently while running. Default: original 15%, translation 70%. At original 0%, hear only translation. Original is live; translated speech is delayed, not synchronized with video.
6. Stop returns the source's native sound and cuts any queued translation tail. Closing the window or Cmd+Q also stops and cleans up first.

Only the selected application's process group is captured. Browsers may have multiple helper processes; selecting Chrome is not selecting a single browser tab. Other programs' sound should remain unaffected. If the source's audio process restarts, refresh and manually start again; the app will not silently attach to a reused PID.

## Вигляд і мова / Appearance and language

У **Налаштування → Вигляд і мова** доступні поточна темна тема та **чорна AMOLED**, довільний колір акценту через палітру або HEX (`#RRGGBB`) і скидання до типового зеленого `#c5e89d`. Текст акцентних кнопок автоматично стає чорним або білим для читабельності. Помилки й попередження зберігають свої семантичні кольори.

**Мова інтерфейсу: Українська / English** перемикається без перезапуску, включно з діагностикою й меню трея; вона не змінює мову перекладу аудіо та не перезапускає захоплення. Типові значення — поточна тема, зелений акцент і українська. Усе автоматично зберігається в тому самому `settings.json`; старі файли отримують нові типові параметри без втрати решти налаштувань.

In **Settings → Appearance and language**, choose Original dark or AMOLED black, any accent with the color picker or a full RGB HEX code, and Ukrainian or English. Changes apply immediately and persist across restarts. Interface language is independent of the audio translation language. External technical errors and application/brand names are preserved. Native window chrome and the system color picker follow the operating system's language and appearance.

## Трей і гаряча клавіша

- Іконка в системному треї (на macOS — у рядку меню) відкриває стандартне меню: мова перекладу, програма, оновлення джерел, старт/стоп, відкрити вікно, вихід. Вибір програми не запускає передавання аудіо. Під час перекладу мова та джерело заблоковані.
- У налаштуваннях натисніть **«Обрати гарячу клавішу»**, введіть модифікатор із клавішею та відпустіть її. Початково глобальна комбінація вимкнена. Escape скасовує запис; конфлікт реєстрації показується поруч із фактично активною комбінацією. Зміни зберігаються автоматично.
- **«Активна програма»** (типовий режим): натискання захоплює програму переднього вікна саме в цей момент, без відкриття Live Translator перед визначенням джерела. **«Остання захоплена»** запускає останнє джерело, чия сесія успішно перейшла в робочий стан; воно запам’ятовується між запусками. Обидва режими використовують поточну мову. Повторне натискання зупиняє поточний переклад, навіть якщо фокус уже на іншій програмі.
- Захоплюється вся програма, не окрема вкладка чи вікно. Власне вікно Live Translator, невідоме або недоступне джерело не підміняються іншою програмою: показується пояснення.
- Кольори: **сірий** — вимкнено; **жовтий** — підключення/зупинка, нестабільний зв’язок або ще немає свіжих вимірювань; **зелений** — активне захоплення зі свіжим відгуком без попередження про мережу; **червоний** — помилка сесії. Текст меню пояснює стан, очікування перекладу й локальну чергу окремо від мережі. Тиша сама по собі не означає проблеми зв’язку.
- Визначення активної програми реалізоване для macOS через NSWorkspace, без автоматизації інтерфейсу. Windows/Linux foreground adapters поки повертають явне повідомлення про відсутність підтримки, без прихованої підміни режиму; аудіозахоплення цих платформ також ще не готове. На Linux доступність трея та глобальних клавіш залежить від середовища, особливо Wayland. За відсутності native-інтеграції основне вікно залишається доступним.
- Згортання вікна не зупиняє керування через трей/клавішу. **Закриття вікна завершує програму**, як і раніше; приховування в трей замість виходу не додавалося. Вихід із трея проходить звичайне збереження й очищення аудіо.

Ця зміна перевіряється автоматизованими тестами й macOS-збіркою. GUI-взаємодія з меню/глобальною клавішею та runtime Windows/Linux потребують окремої ручної перевірки; не вважайте їх перевіреними лише за результатами тестів.

## Settings and privacy

Config path is obtained from Tauri `app_config_dir()`; on macOS:

```text
~/Library/Application Support/dev.live-translator.desktop/settings.json
```

Per the user's explicit preference, **the API key is plaintext in this file**, not in Keychain. Unix directory/file permissions are 0700/0600; this is not encryption. Do not share/back up this file to a public repository. No key is embedded in the app, diagnostics or test fixtures. Enter it through the UI; the app does not inspect chat history or environment variables for credentials.

Source audio is transmitted to **Google** only after explicit start. It is not saved to local audio files. Google free-tier availability and quotas depend on the project/account; free-tier data may be used to improve Google's products under its terms. The app never enables billing or automatically changes providers/tariffs. A key for a billed project still uses that project's billing policy.

Model: `gemini-3.5-live-translate-preview`, official bidirectional Live API. First target languages in the UI: Ukrainian, English, Polish, German. Input: PCM16 mono 16 kHz in 100ms chunks; output: PCM16 mono 24 kHz. Current capture supports Float32 mono/stereo at 44.1/48 kHz, and common output PCM formats; other capture layouts/rates fail clearly instead of being reinterpreted.

## Діагностика затримки

Під час перекладу компактна панель показує **останні 10 вимірювань пінгу** та історію **останніх 10 секунд для черг**:

- **Відгук Google (мс)** — Ping → Pong поточного WebSocket-з’єднання, не швидкість завантаження і не час роботи моделі. Контрольний Ping надсилається приблизно раз на 4 секунди; кожен стовпчик графіка відповідає одному фактичному вимірюванню. Стовпчики розташовані щільно поруч і зсуваються тільки після нового результату, а не з плином часу. Історія обмежена 10 результатами; давні стовпчики залишаються видимими, проте поточне значення та попередження враховують лише свіжі дані (до 10 секунд). Відсутні/прострочені відповіді — «Немає даних», не нуль і не доказ поганого інтернету. Повільне локальне завершення запису Ping (>100 мс) робить RTT неоднозначним, тому такий результат не використовується.
- **Черга надсилання (с)** — захоплене аудіо, що чекає передачі: вхідне кільце, неповна порція, черга відправника і поточне надсилання. Окремо показано тривалість останньої операції запису; її завершення не доводить отримання/обробку Google.
- **Черга озвучення (с)** — вже отриманий переклад у всіх локальних буферах, що чекає відтворення. Це лише одна складова затримки, не відставання від відео.

Кожен рядок має власну підписану шкалу. Пояснення можуть називати кілька спостережуваних проблем одночасно, не приписуючи їх однозначно інтернету чи Google. Пороги — евристики: відгук у середньому ≥500 мс або розмах ≥250 мс за щонайменше трьома свіжими відповідями; черга надсилання ≥0,5 с або озвучення ≥2 с протягом щонайменше 2 с. Одиничний стрибок черги чи тиша не вважаються несправністю.

Історія (10 вимірювань пінгу та 10 секунд черг) зберігається лише в пам’яті, скидається між сесіями та не містить аудіо, ключа або тексту перекладу. Додаткова Gemini-сесія не відкривається. Надсилання аудіо виконується окремим асинхронним завданням із обмеженою чергою (8 порцій по 100 мс плюс одна в роботі), щоб повільний запис не блокував приймання перекладу та локальне відтворення. Переповнення зупиняє сесію явно, не відкидає слова мовчки. Модель, розмір порцій, початковий буфер і політика прискорення не змінені.

**Точну синхронізацію з відео ця панель не вимірює і сама по собі не виправляє.**

## Failure policy

- One active session. Source and language are locked until stopped. New key applies next start; gains apply immediately.
- Total network/setup deadline 15s, cancellable connect/read/write. Cleanup target 3s; unusually blocked OS calls cannot be forcibly interrupted safely, so the app reports a timeout rather than pretending cleanup succeeded.
- Input ring approximately 1s. Server audio packets are staged and fed incrementally into the output ring (target ≤2s, physical capacity 5s), so one packet no longer has to fit the remaining ring space atomically. Local staged+queued audio is bounded at 15s; the worker keeps receiving and accounts for new packets even while staged audio remains, so overload cannot be hidden in an unread transport backlog. Capture/upload and playback processing continue fairly until a protective limit or error. No words are silently discarded. The WebSocket parser separately limits a frame to 1 MiB.
- When backlog grows, translated playback gently accelerates up to 1.12×; original playback stays at 1×. This simple interpolation-based catch-up can slightly change pitch; it is not pitch-preserving time stretching. Current tempo is always shown in session metrics, including 1.00×, without appearing/disappearing as catch-up changes. A brief >5s burst is tolerated; >5s backlog sustained for 20s or a >15s local total still stops clearly to avoid runaway delay.
- No blind reconnect after quota/network/session-limit errors: original returns, and the user chooses when to start again. Diagnostics distinguish the failed stage (connect/setup/send/receive), safe transport categories (TLS/reset/timeout) and explicit server availability codes. Server busy/1013 and internal/1011 failures are not mislabeled as exhausted quota; only explicit 429 indicates quota.
- Input level reflects actual capture samples; first-chunk timing and buffer length are not semantic translation latency measurements.
- Output device changes/disconnects require a new manual start. Manual output device selection is not in v0.1.
- Forced process kills/crashes are outside graceful-close guarantees. Native resources are private/process-scoped, but verify sound returns after an OS-level failure. An exceptional failed native callback shutdown retains its context instead of risking use-after-free and reports cleanup failure.

## Verify

```sh
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
npm run check
npm test
npm run build
npm audit --audit-level=moderate
```

Tests use temporary settings files, local WebSockets and fake session runners; no real key, paid requests or captured audio. An explicit CLI equivalent of the Settings connection check is available without GUI automation:

```sh
cargo run -p translator-core --example check_connection -- "$HOME/Library/Application Support/dev.live-translator.desktop"
```

It reads the key only from that explicitly supplied settings directory, prints a safe result, opens/closes a real Google Live session, and sends no audio. Manual checklist: local `docs/testing/macos-smoke.md` (development notes are intentionally not tracked or published in Git).

Local design, implementation plans and verification notes live in the ignored `docs/` directory; they are not part of repository history or release source archives. Repository: https://github.com/kvxshenyjbetxn/live.translator. Release versions follow SemVer; Rust workspace, frontend, Tauri metadata and bundled app must agree. Earlier testing reports describe their historical build versions.
