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

## Keyboard Workflow

- `F2`: Rename
- `F3`: Open a console in the active panel path
- `F5`: Copy to the opposite panel
- `F6`: Move to the opposite panel
- `F8`: Delete
- `Tab`: Switch active panel
- `Enter`: Open directory or launch file with the default app

Long copy/move/delete operations run off the GTK main loop and report progress through a GTK progress dialog. Conflicts can be resolved with overwrite, skip, rename, or cancel.
