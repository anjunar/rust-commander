# RCommander GTK4 Rebuild Architecture

This project is a clean GTK4 rebuild. The old Slint prototype is used only as a source of domain behavior: panel state, directory entries, sorting, roots, file operations, watcher behavior, and OS open actions. Slint UI files, Slint models, callbacks, and the homemade table abstractions are intentionally not reused.

## Module Responsibilities

- `domain/`: GTK-free data and rules. It owns file entries, panel navigation state, selection state, sorting state, root descriptors, and file operation request/result/event types.
- `application/`: GTK-widget-free command coordination. It owns `AppState`, decides which panel is active, builds copy/move/delete requests, updates status text, calls filesystem/platform services, and returns explicit `ViewUpdate` values.
- `fs/`: real IO. It reads directories, formats metadata, performs copy/move/delete/rename work, emits progress/conflict events, and watches visible directories.
- `platform/`: OS-specific behavior. It detects roots/drives, opens files with the default app, and provides icon-name hints for GTK cells.
- `ui/`: GTK-only code. It creates the `gtk::Application`, main window, two commander panels, `ColumnView` columns/factories, `glib::Object` row wrappers, dialogs, shortcuts, CSS, and event wiring.

## Data Flow

1. GTK events and shortcuts call small UI closures.
2. UI closures call `application::Commander` commands.
3. Commands mutate `AppState` and return `ViewUpdate`.
4. The UI applies the update by remapping domain `Entry` values into GTK `FileRowObject` rows.
5. Filesystem changes and operation progress are delivered back to the GTK main loop and applied through the same update path.

Business rules stay outside GTK callbacks. GTK callbacks only translate user gestures into application commands and then refresh the view.

## GTK Model/View Mapping

Each panel owns:

- a `gio::ListStore<FileRowObject>` containing UI row objects,
- a `gtk::MultiSelection` wrapping that store,
- a `gtk::ColumnView` using native GTK columns and headers,
- `gtk::SignalListItemFactory` instances for the `Name`, `Size`, `Type`, `Modified`, and `Attributes` columns.

`domain::Entry` remains GTK-free. `ui/file_row_object.rs` maps each `Entry` into a small `glib::Object` wrapper that factories can bind to GTK labels/images. The file table is not manually drawn and rows are not simulated with boxes outside the native `ColumnView` cell factories.

## Threading Model

Directory reads are synchronous for now because they are small command operations and are surfaced with status updates. Long copy/move/delete operations run on background threads in `fs::operations`. They send `OperationEvent` values through channels. The GTK layer polls those receivers from the main loop with `glib::timeout_add_local`, updates progress dialogs on the GTK thread, and forwards conflict resolutions/cancellation back through an `OperationHandle`.

The filesystem watcher runs on its own thread via `notify`. It sends refresh notifications to the GTK main loop, where the application refreshes visible panels safely.

## Reused, Moved, Deleted

- Reused and moved:
  - old `panel::Entry` became `domain::entry::Entry`;
  - old selection logic became `domain::selection`;
  - old panel navigation state became `domain::panel`;
  - old sorting became `domain::sorting` with no Slint sort type;
  - old operation request/events and background implementation became `domain::operation` and `fs::operations`;
  - old watcher behavior became `fs::watcher`;
  - old OS open/root behavior became `platform::*`.
- Rebuilt:
  - application state and command dispatch are now in `application/`;
  - the entire UI is GTK4 with `ColumnView`, `gio::ListStore`, `MultiSelection`, actions, shortcuts, and GTK dialogs.
- Deleted/not carried forward:
  - Slint dependency and build integration;
  - `.slint` UI files;
  - Slint `TableRow`/`TableCell`/callback model concepts;
  - manual table layout and visual column width calculation.
