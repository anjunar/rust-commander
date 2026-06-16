# RCommander

RCommander is a GTK4 Rust rebuild of a two-pane Norton Commander style file manager.

## Build

```powershell
cargo check
cargo run
```

## GTK4 Runtime Notes

This project uses gtk-rs (`gtk4`, `gio`, `glib`) and native GTK4 widgets. The file panels are backed by `gio::ListStore`, `gtk::MultiSelection`, and `gtk::ColumnView`; rows are rendered through `gtk::SignalListItemFactory`.

On Windows, make sure the GTK4 runtime and development files are installed and visible to `pkg-config` before building. The project targets GTK 4.10 APIs for `ColumnView` sorting integration.

## Embedded Terminal Notes

The bottom terminal dock is integrated into the GTK4 layout and uses a real `vte4` terminal on Linux builds.

On Windows, the dock currently remains a structured placeholder instead of pretending to be a terminal. This is intentional: Microsoft ConPTY provides a pseudoconsole stream, but the host application must still render terminal output and collect terminal input itself. GTK4 does not provide a native Windows terminal widget comparable to VTE, and RCommander explicitly avoids shipping a homemade textbox-based terminal emulator.

The current architecture already isolates the terminal backend so a future Windows-native terminal control can be added without rebuilding the surrounding commander UI.

The selected Windows direction is documented in [WINDOWS_TERMINAL_STRATEGY.md](./WINDOWS_TERMINAL_STRATEGY.md).

## Keyboard Workflow

- `F2`: Rename
- `F3`: View the selected file, including binary files as hex
- `F4`: Edit UTF-8 text files
- `F5`: Copy to the opposite panel
- `F6`: Move to the opposite panel
- `F8`: Delete
- `Tab`: Switch active panel
- `Enter`: Open directory or launch file with the default app

Long copy/move/delete operations run off the GTK main loop and report progress through a GTK progress dialog. Conflicts can be resolved with overwrite, skip, rename, or cancel.
