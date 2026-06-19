# UI Architecture

`MainWindow` is the composition root for the GTK shell.

Responsibilities:

- `main_window.rs`: widget construction, controller wiring, and cross-cutting UI updates.
- `main_window_navigation.rs`: navigation flow, directory loading, refresh scheduling, and watcher-driven reloads.
- `main_window_operations.rs`: file/archive operation startup, progress polling, and completion handling.
- `main_window_panel_wiring.rs`: panel callback wiring and dispatch from GTK events into commander/navigation actions.
- `main_window_context_menu.rs`: panel context-menu selection prep and platform-specific menu handling.
- `main_window_terminal.rs`: terminal dock event wiring and terminal-specific UI reactions.
- `main_window_window_chrome.rs`: theme application, localized chrome refresh, split initialization, and window state persistence.
- `main_window_actions.rs`: command handlers that translate UI intents into controller calls or small local UI actions.

Rules for future changes:

- Prefer the narrowest controller that already owns the required state.
- Do not add new business logic directly to `MainWindow` unless it is widget composition or wiring.
- Controllers should depend on specific `Rc<RefCell<...>>` handles plus a small host trait, not on `Rc<MainWindow>`.
- If a callback only needs navigation or operation behavior, capture that controller instead of cloning the full window.
