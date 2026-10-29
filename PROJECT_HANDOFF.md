# Wallmus — Handoff Context & Session State

Этот документ содержит полную сводку текущего состояния проекта **wallmus**, архитектуру, историю решённых проблем и инструкции для ИИ-агента в **Antigravity 2.0**.

---

## 1. О проекте (Краткая суть)
**wallmus** — приложение для Windows 10/11 для живых анимированных обоев рабочего стола, синхронизированных с музыкой, с плавающим полупрозрачным нативным виджетом управления треками.

### Архитектурные компоненты (Rust + Tauri):
1. **`crates/desktop-host`**: Встраивание обоев между рабочим столом и иконками через Win32 `WorkerW` / `Progman` (SpawnWorker). Двойной буфер для плавного кроссфейда (DirectComposition / Direct2D / WIC).
2. **`crates/audio-engine`**:
   - `smtc.rs`: Захват метаданных текущего трека (Яндекс Музыка, Spotify, браузеры, VK) через Windows Runtime `GlobalSystemMediaTransportControlsSessionManager`.
   - `visualizer.rs`: Захват реального аудиопотока через WASAPI loopback (`IAudioCaptureClient`) с FFT-анализом спектра на лету.
   - `player.rs`: Встроенный локальный аудиоплеер (rodio).
3. **`crates/widget`**: Нативный легковесный оверлей-виджет на рабочем столе (`WS_EX_LAYERED` с `UpdateLayeredWindow` и 32-bit ARGB / GDI+).
   - Темы: `spotify`, `pill`, `vinyl` (с вращающейся виниловой пластинкой), `glass`.
   - Адаптивная масштабируемая раскладка (`Layout::compute`), hover-эффекты, перетаскивание за любое место (`HTCAPTION`), изменение размера мышкой за правый нижний угол (`HTBOTTOMRIGHT`).
4. **`crates/governor`**: Модуль энергосбережения и оптимизации (WinEvent hook): автоматически ставит рендеринг обоев на паузу при полноэкранных приложениях, блокировке экрана или выключении дисплея (0% CPU / EcoQoS).
5. **`crates/daemon` (`wallmus.exe`)**: Главный процесс службы, системный трей, Named Pipe IPC сервер (`\\.\pipe\wallmus-ipc-v1`).
6. **`ui/` (`wallmus-ui.exe`)**: Интерфейс настроек и библиотека обоев на React + Vite + TypeScript + Tauri v2.

---

## 2. Что было сделано и исправлено в последней сессии:

### Проблема 1: Исчезновение виджета и отсутствие связи с интерфейсом
- **Причина**: Нажатие на старый крестик (`✕`) на виджете сохраняло в `config.toml` значение `[widget] enabled = false`. При перезапуске служба `wallmus.exe` инициализировала виджет один раз до главного цикла и больше не проверяла флаг включения.
- **Исправление**:
  1. В `crates/daemon/src/main.rs` добавлен динамический жизненный цикл: цикл отслеживает `config.widget.enabled` и на лету создаёт/уничтожает экземпляр `WidgetController`.
  2. В IPC добавлен автозапуск виджета при открытии интерфейса (`app.openUi`).
  3. Закрытие окна настроек оставляет службу и виджет активными на рабочем столе.

### Проблема 2: Убран сбивающий с толку крестик (`✕`)
- С виджета удалена кнопка закрытия. Скрыть или включить виджет теперь можно:
  - В окне настроек переключателем «Виджет на рабочем столе».
  - Правым кликом мыши по виджету -> *«Скрыть виджет»*.
  - Глобальной горячей клавишей `Ctrl+Alt+M`.

### Проблема 3: Полная адаптивность виджета под любые размеры
- В `crates/widget/src/painter.rs` и `crates/widget/src/gdi_plus.rs`:
  - Вся раскладка переведена на относительные пропорции (`Layout::compute(theme, w, h)`).
  - Шрифты текста (название трека, исполнитель), размер скруглённой обложки, кнопки управления (Play/Pause, Prev, Next, Volume) и полоса прогресса масштабируются синхронно.
  - Устранена проблема острых углов обложки (теперь маска точно соответствует радиусу виджета).
  - Для темы **Vinyl** обложка скругляется в идеальный круг с центром-отверстием и плавно вращается в таккт воспроизведению.
- В окне настроек (`ui/src/views/SettingsView.tsx`):
  - Добавлены пресеты размеров: **S** (300px), **M** (380px), **L** (500px), **XL** (660px).
  - Добавлен ползунок непрерывного изменения ширины (от 260px до 900px) и ползунок прозрачности (от 30% до 100%).
  - Добавлены IPC-методы `widget.setSize` и `widget.setOpacity`.

### Проблема 4: Адаптивность окна настроек (UI)
- В `ui/src/index.css` добавлены медиа-запросы для экранов с небольшим разрешением и высоким масштабированием (125%-150%). Сайдбар и панели больше не наезжают друг на друга.

### Проблема 5: Исправление запуска UI (`wallmus-ui.exe`) на рабочем столе `WinSta0\Default` и live-синхронизация
- **Причина неоткрытия окна**: При запуске `wallmus.exe` из изолированного процесса (`exebox-*`), `SetThreadDesktop` менял рабочий стол только главного потока, а фоновые потоки (`launch_or_focus_ui`) и дочерний `wallmus-ui.exe` наследовали скрытый рабочий стол `exebox-*` и блокировали профиль WebView2.
- **Решение**:
  - В `crates/desktop-host/src/topology.rs` добавлены `is_on_default_desktop()` и `spawn_on_default_desktop()` (через `CreateProcessW` с `STARTUPINFOW.lpDesktop = "winsta0\\default"`).
  - В `crates/daemon/src/main.rs` демон автоматически перезапускается на `WinSta0\Default`, если был вызван с не-дефолтного рабочего стола; поток `launch_or_focus_ui()` явно привязывается к `Default` и запускает `wallmus-ui.exe` через `spawn_on_default_desktop`.
  - В `ui/src-tauri/src/lib.rs` добавлены `ensure_ui_default_desktop()`, защита от дублирования процесса `wallmus-ui.exe` (`focus_existing_ui_window()`) и автозапуск демона (`ensure_daemon_running()`), если пользователь открыл `wallmus-ui.exe` напрямую.
- **Live-синхронизация виджета и UI**:
  - В `WidgetController` (`crates/widget/src/controller.rs`) добавлен метод `apply_config()`, применяющий тему, размеры, позицию и прозрачность на лету при изменениях из настроек UI (`pending_widget_sync`).
  - В тему `pill` (`crates/widget/src/painter.rs`) добавлены кнопка настроек (`⋯`) и уголок изменения размера мышью (`grip`).
  - Добавлены IPC-команды `app.openLogs`, `app.pickWallpaperFile`, `app.pickMusicFolder`, `wallpapers.delete`, `governor.setConfig`, `settings.setFpsLimit`, `settings.setGeneral`, `tracks.list`, а также конвертация локальных превью обоев и обложек треков в `data:image/...;base64,...` URI для корректного отображения в WebView2.

---

## 3. Сборка и размещение бинарников
- **Frontend**: собран через `npm run build` в `ui/dist`.
- **Release бинарники**:
  - `target/release/wallmus.exe`
  - `target/release/wallmus-ui.exe`
  - `target/release/wallmus-htmlhost.exe`
- **Рабочая директория дистрибутива**: `dist/wallmus/` (и скопированы в корень `dist/`, включая `libmpv-2.dll`).

---

## 4. Конфигурация и пути
- Файл настроек: `%APPDATA%\wallmus\config.toml`
- База данных (SQLite): `%APPDATA%\wallmus\wallmus.db`
- Логи: `%LOCALAPPDATA%\wallmus\logs\wallmus.log.<YYYY-MM-DD>`
- Именованный пайп IPC: `\\.\pipe\wallmus-ipc-v1`
  - Формат сообщений: JSON Lines, поля `{"v": 1, "id": <num>, "method": "...", "params": {...}}`.
