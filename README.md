# rust_taskbar

Custom Windows taskbar replacement written in Rust. Slint (winit, software renderer) draws the
UI; Win32 calls handle windows, tray, audio, network and input. Native taskbars are hidden while it
runs and restored on quit (a watchdog process restores them if it crashes).

## Layout

| Path | Purpose |
|------|---------|
| `src/main.rs` | startup, 500 ms tick, refresh |
| `src/shell.rs`, `tasks/` | bars per monitor, window list, button layout (`push_ui`) |
| `src/tray/` | own `Shell_TrayWnd`, tray icon model, overflow flyout |
| `src/popup/`, `menu.rs`, `calendar.rs`, `start/` | popup windows, context menus, calendar, Start launcher |
| `src/quick/` | quick settings: audio + mixer, Wi-Fi, Bluetooth, keyboard language (`lists.rs` drop-down rows, `wiring.rs` callbacks) |
| `src/settings/` | Settings window wiring (themes, bar, tray, monitors, profiles, services in `svc.rs`) |
| `src/services.rs`, `webservices/` | built-in web services (Gmail, Outlook, Messenger, Teams) in WebView2: bar groups, badges, flashing, mail lists/panel, schedule; per-site adapters in `webservices/src/adapters/*.js` read run-time options from `window.__opts` |
| `src/config/` | `config.toml`, live reload, profiles |
| `src/theme.rs` | pushes theme values to every Slint window |
| `ui/*.slint` | Slint sources; `settings*.slint` and `quick*.slint` split into widgets, previews and the window |
| `winkey_hook/` | DLL injected into the Start menu hosts to block the native Start menu |

## Build and run

```
cargo build --release --workspace
target\release\rust_taskbar.exe
```

If the build fails with "failed to remove winkey_hook.dll", `StartMenuExperienceHost.exe` still has
the hook loaded. Stop that process (Windows restarts it) and build again.

## Data

All under `%APPDATA%\rust_taskbar\`:

- `config.toml`: all settings; edits made by hand are picked up live.
- `profiles\*.toml`: saved setting profiles (Settings > Profiles).
- `log.txt`: errors such as failed saves or an invalid `config.toml` (restarts after 256 KB).

## Conventions

- Single UI thread; global state is in `thread_local!` cells. Do not hold a `borrow_mut()` across calls
  that can re-enter.
- Config edits go through `update_ui` / `update_theme` / `update_monitor`; do not clone, edit and save by hand.
- Slint popup windows share `Theme.popup-surface` as their background so they repaint reliably.
