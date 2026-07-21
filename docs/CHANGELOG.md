# All project changes (uncommitted)

Summary date: 2026-07-21  
Comparison base: `master` @ `34e471a` (`edit: design`)  
Scope: **~87 modified tracked files**, **+9031 / −1458** lines (excluding `Cargo.lock` and `target*`), plus **~18 new source/resource files**.

This document lists **all** working-tree changes relative to the latest commit.  
Russian version: [`CHANGES-all-ru.md`](CHANGES-all-ru.md).

---

## Topic summary

| Topic | Summary |
|------|---------|
| **7z** | New backend (`sevenz-rust2`) + out-of-process worker on Windows |
| **ZIP names** | Unicode Path + CP866/CP1251 heuristics |
| **Tabs** | Domain + AppState + UI + config persistence |
| **Marks / cursor** | Classic Total Commander model (cursor ≠ marks) |
| **Folder sort** | Remember sort order per folder |
| **Find files** | Domain API + FS worker + Alt+F7 dialog |
| **Favorites** | Dialog, Ctrl+D, Move up/down, compact list |
| **Properties** | Windows Explorer / built-in dialog / SFTP chmod |
| **Menu** | Files / Commands / View / Help |
| **Editor** | Independent window, Find/Replace, remote edit, nginx.lang |
| **SSH** | Trust host key, listing cache, ssh-agent fallback |
| **Symlinks / .lnk** | Classification and safe navigation |
| **Windows safety** | Do not open `.7z` in the UI process; icons/open out-of-process |
| **Russian locale** | New `locales/ru.yml` |
| **Themes / command bar** | Tabs, footer, marked rows, F-keys with modifiers |
| **README** | Updated status, shortcuts, archives, SFTP |

---

## Dependencies (`Cargo.toml` / `Cargo.lock`)

Added:
- `sevenz-rust2 = "0.20"` — 7-Zip support
- `encoding_rs = "0.8"` — ZIP name decoding
- `regex = "1.11"` — find-files (regex)

Windows:
- `windows` crate gained feature `Win32_Storage_FileSystem`

`Cargo.lock` updated accordingly.

---

## New files

### Source
| File | Purpose |
|------|---------|
| `src/archive/sevenz_backend.rs` | Backend list/extract for `.7z` |
| `src/archive/sevenz_worker.rs` | CLI worker process (Windows OOP) |
| `src/archive/zip_name.rs` | ZIP name decoding |
| `src/domain/panel_tabs.rs` | Panel tabs model |
| `src/domain/folder_sort.rs` | Per-folder sort preferences |
| `src/domain/search.rs` | Find-files model (glob/regex) |
| `src/fs/links.rs` | Symlink / `.lnk` / junction navigation |
| `src/fs/search.rs` | Background cancellable search walk |
| `src/fs/temp.rs` | Temp dir for remote edit |
| `src/ui/dialogs_favorites.rs` | Favorites dialog |
| `src/ui/dialogs_properties.rs` | Properties dialog (local + SFTP) |
| `src/ui/dialogs_search.rs` | Find-files dialog |
| `src/ui/main_window_menu.rs` | Application menu |

### Locales / assets / tests / docs
| File | Purpose |
|------|---------|
| `locales/ru.yml` | Full Russian locale |
| `assets/gtksourceview-5/language-specs/nginx.lang` | nginx highlighting in the editor |
| `tests/unit/archive_sevenz_tests.rs` | 7z tests |
| `tests/unit/domain_folder_sort_tests.rs` | Folder sort tests |
| `tests/unit/domain_panel_tabs_tests.rs` | Tabs tests |
| `docs/CHANGES-all-ru.md` | This summary (RU) |
| `docs/CHANGES-all-en.md` | This summary (EN) |

### Removed
| File | Reason |
|------|--------|
| `tests/unit/platform_icons_windows_tests.rs` | Nested include removed; cases moved/simplified |

---

## 1. Archives (`src/archive/**`)

### 1.1. 7-Zip support
- Format `ArchiveFormat::SevenZip` / family `ArchiveFamily::SevenZip`.
- `SevenZipBackend` — list/extract, error mapping (password, CRC, I/O).
- On **Windows**, list/extract run in a **separate process** (`--archive-worker`) so the 7-Zip shell extension is not loaded into the UI (avoids `STATUS_STACK_BUFFER_OVERRUN`).
- `sevenz_worker.rs` — stdout protocol with `\x1f` separator (`list` / `extract`).
- `archive_worker_main()` in `archive/mod.rs`; early exit from `main.rs` / re-export in `lib.rs` without GTK.

### 1.2. ZIP names
- `zip_name.rs`: Info-ZIP Unicode Path (0x7075) → UTF-8 → CP866/CP1251 heuristics → crate fallback.
- Uses `encoding_rs`.

### 1.3. Detection and registry
- Detection: **extension first**, then magic bytes; a known suffix (including `.7z`) **does not open the file** solely for detection.
- `is_archive_path` / Enter: extension-first — `.7z` on Windows does not go through `read_dir`/shell.
- `AskArchiveAction` → **`OpenArchive`** (open archive directly, no “as archive / externally” choice).
- `resolve_for_path` → `backend_that_can_open`; separate `probe_path()` / `can_open_path()` for “not supported yet” UX.
- Multipart ZIP probe: suffix by **bytes**, not char-slice (fixes panic on Cyrillic names).
- Shared helpers: `normalize_archive_entry_path`, `archive_entry_display_name`, `archive_entry_matches_selection`.
- Zip/Unrar moved to shared path helpers; single-entry extract → via `extract_entries`.

### Modified files
`detector.rs`, `mod.rs`, `path.rs`, `probe.rs`, `registry.rs`, `service.rs`, `unrar.rs`, `zip_backend.rs`  
New: `sevenz_backend.rs`, `sevenz_worker.rs`, `zip_name.rs`

---

## 2. Domain (`src/domain/**`)

### 2.1. Tabs — `panel_tabs.rs`
- `PanelTabs`, `TabId`
- open / close / activate / next / prev

### 2.2. Folder sort — `folder_sort.rs`
- Path normalization, lookup/upsert/remove
- Session override wins over config preference

### 2.3. Search — `search.rs`
- `SearchOptions`, `SearchHit`, `CompiledSearch`
- glob (`*` / `?`) and regex, exclude masks

### 2.4. Panel / selection / sorting / location
- Cursor ≠ marks: `SelectionModel::single` no longer marks the row; marks are independent.
- `selected_items()`: if there are no marks — uses the **cursor** (classic commander).
- Mark API: `toggle_mark`, `toggle_mark_in_place`, `mark_all`, `invert_marks`, `set_cursor`, `set_marks`.
- `replace_entries` / `navigate_to` **do not sort** — sorting runs on the worker.
- On navigate, `entry_key_when_leaving_to` is used (focus the folder just left).
- Directory size calculation: `calculated_dir_sizes`, `set_calculated_dir_size`.
- Quick-search: `index_matching_name_prefix`.
- `PanelLocation`: `tab_title()`, `root()`, `entry_key_when_leaving_to()`; FS `parent()` at root → `None`.
- **Type** sort — by **file extension**, not `EntryKind`.
- `SortColumn` / `SortDirection`: `Serialize`/`Deserialize`, `Default`.
- `folders_first` made `pub`.

### Modified / new
Modified: `mod.rs`, `panel.rs`, `panel_location.rs`, `selection.rs`, `sorting.rs`  
New: `panel_tabs.rs`, `folder_sort.rs`, `search.rs`

---

## 3. Filesystem (`src/fs/**`)

### Added
- **`links.rs`**: symlink / Windows `.lnk`; `resolve_directory_navigation_path` (junction fallback, e.g. `Documents and Settings` → `Users`).
- **`search.rs`**: background cancellable walk → `SearchEvent` (Progress / Hits / Failed / Finished).
- **`temp.rs`**: temp dir for remote edit.
- `directory_total_size()` — recursive size sum (does not enter nested symlink dirs).
- `copy_entry()` — recursive tree copy.
- Single-source transfer: optional explicit `target_path`.

### Changed
- Listing via `classify_fs_entry`; `read_entry` → `symlink_metadata`.
- Attributes: `L` flag for links; on Unix symlink is detected before directory.
- Navigating into a “folder” resolves the path via `resolve_directory_navigation_path`.

### Files
Modified: `mod.rs`, `operations.rs`, `reader.rs`  
New: `links.rs`, `search.rs`, `temp.rs`

---

## 4. Application (`src/application/**`)

### Added
- Tabs: `open_tab` / `close_tab` / `activate_tab` / `next_tab` / `prev_tab` / `restore_tabs` / `swap_panels`.
- Marks + quick search: `set_cursor`, `toggle_mark*`, `mark_all`, `invert_marks`, `quick_search`, `set_calculated_dir_size`.
- `CreateDirectoryTarget` — FS **or** Remote.
- `create_file_request`, `copy_with_rename_request`.
- Navigation: `parent_navigation_request`, `filesystem_navigation_request`, `location_root_navigation_request`.
- `DeleteMode` (`Default` | `Permanent`); `prepare_operation_with_options`.
- `NavigationError::HostKeyTrustRequired`.

### Changed
- `AppState.left/right`: `Panel` → **`PanelTabs`**; `panel()` = active tab.
- `NavigationRequest` / `DirectoryLoadResult`: `tab_id`, `sort_state`, `folders_first`; sort on worker.
- Archives: `AskArchiveAction` → `OpenArchive`; archive check **before** `is_dir`.
- `ViewUpdate::selection`: updates panel entries **without** changing `active_panel` (less scroll jumping).
- After Copy: clear marks immediately; Delete/Move — queue delete selection.
- Remote ops: `force_overwrite`; Local: `target_path`.
- `EntryLoader`: `force_refresh` + `session_key`; untrusted host key mapping.

### Files
`app_state.rs`, `commander.rs`, `commands.rs`, `errors.rs`, `mod.rs`, `navigation.rs`, `operation.rs`, `operation_runner.rs`, `services/entry_loader.rs`

---

## 5. Config (`src/config.rs`)

Added:
- `favorites: Vec<FavoriteEntry>`
- `folder_sort: Vec<FolderSortPreference>`
- `EditorConfig` (window size, max edit size MiB)
- `IconStyle` (`Symbolic` | `System`) in `GeneralConfig`
- `PaneTabConfig` + `left_tabs` / `right_tabs` / `*_active_tab`
- Legacy fields: `left_cursor` / `right_cursor`

Behavior: old configs without new fields load with `Default`; tabs/cursors/favorites/folder_sort serialize to TOML.

---

## 6. Remote / SFTP (`src/remote/**`)

### Added
- Interactive host key trust: `HostKeyTrustOffer`, `HostKeyUntrusted`, `RemoteService::trust_host_key`, write to OpenSSH `known_hosts`.
- SFTP listing cache (`listing_cache`); `invalidate_listing_cache_for_path`; `force_refresh` bypasses cache.
- `create_directory`, `set_permissions`.
- Symlinks in remote listing (`EntryKind::Symlink`, follow for is_dir).
- `force_overwrite` for download/upload.
- Auth: **ssh-agent** fallback after pubkey file failure (important for Ed25519 on Windows/WinCNG).

### Changed
- Missing `known_hosts` / NotFound → `HostKeyUntrusted` (UI can offer trust), not a hard bail.
- `read_entries` takes `session_key` + `force_refresh`.

### Files
`mod.rs`, `service.rs`

---

## 7. Platform (`src/platform/**`)

### Added
- `IconStyle` runtime + `set_icon_style` / `icon_for_listing_entry` / `fallback_icon_name_for_row`.
- Windows: `show_file_properties`, `force_foreground_window`, `resolve_shortcut_target` (COM `.lnk`).
- `open_path` via **out-of-process** `cmd /C start` (not in-process ShellExecute).

### Changed
- List icons: extension/directory hints only + `USEFILEATTRIBUTES` — **do not open** live paths like `.7z`.
- Removed `platform_icons_windows_tests.rs`.

### Files
`icons.rs`, `mod.rs`, `windows.rs`

---

## 8. UI — panel tabs

**Files:** `file_panel_view.rs`, `commander_view.rs`, `main_window.rs`, `main_window_actions.rs`, `main_window_panel_wiring.rs`, `main_window_window_state.rs`, `shortcuts.rs`, `theme_*.css`

- Tab strip (`.panel-tab-bar` / `.panel-tab` / `.panel-tab-add`) + “+” button.
- Actions: new / close / next / prev.
- Shortcuts: `Ctrl+T`, `Ctrl+W`, `Ctrl+Tab` / `Ctrl+PageDown`, `Ctrl+Shift+Tab` / `Ctrl+PageUp`.
- Session: save/restore `left_tabs` / `right_tabs` and active index (`persist_pane_session`).
- Sort and cursor are **per tab**.

---

## 9. UI — application menu

**New:** `src/ui/main_window_menu.rs`  
**Related:** `main_window.rs`, `main_window_window_chrome.rs`, `shortcuts.rs`

`PopoverMenuBar`:

| Menu | Items |
|------|-------|
| **Files** | Open, View, Edit, Properties, Rename; New file / Directory; Tabs; Favorites / SFTP; Quit |
| **Commands** | Copy / Copy as / Move / Delete / Wipe; Search; Select all; Swap panels; Terminal |
| **View** | Sort options; Remember/forget folder sort; Refresh; Hidden files; Drives; Root |
| **Help** | Settings, About |

`refresh_menu_bar` updates labels on language change.

---

## 10. UI — find files

**New:** `src/ui/dialogs_search.rs`  
**Related:** `dialogs.rs`, `main_window_actions.rs`, `shortcuts.rs`, keys `search.*`

- Find files dialog (`Alt+F7` / `Ctrl+Shift+F`): name pattern, exclude, directory, subdirs, case, regex.
- Results in `ColumnView` (name, type, path, size, date); Start/Stop; statuses.
- Local directories only; **GoTo** opens the result folder and focuses the file.
- Exports: `show_find_files`, `SearchDialogAction`, `search_go_to_request`.

---

## 11. UI — file properties

**New:** `src/ui/dialogs_properties.rs`  
**Related:** `main_window_actions_file.rs`, `shortcuts.rs` (`Alt+Enter`)

- **Windows + local FS:** native Explorer property sheet.
- **Linux/macOS / archives:** built-in dialog (name, path, type, size, dates, attributes).
- **SFTP:** `show_remote_properties` — octal + owner/group/other (rwx) checkboxes, Apply.

---

## 12. UI — favorites

**New module** `dialogs_favorites.rs` (wired in `dialogs.rs`)

### Full feature
- Favorites list; Add current / Edit / Delete / Go; name+path dialog.
- Stored via `FavoritesAction::Update`; navigation via `FavoritesAction::Go`.
- Shortcut **`Ctrl+D`**; menu item.

### Order and compact list
- **Move up / Move down** buttons (`favorites_move_up` / `favorites_move_down`) after Delete.
- `move_selected(delta)`: swap neighbors, persist via `FavoritesAction::Update`, move focus.
- Compact rows: **name only**, path in tooltip, smaller padding, no `dialog-title`.
- DnD not implemented; `FavoriteEntry` config format unchanged.

---

## 13. UI — selection, marks, footer, quick-search

**Files:** `file_panel_view.rs`, `columns.rs`, `file_row_object.rs`, `presentation.rs`, `commander_view.rs`, `main_window_panel_wiring.rs`, `main_window_hosts_impl.rs`

- Multi-selection → **cursor + marks** (`SingleSelection`); marked — red `#ff4d4d` + CSS `.marked`.
- **Insert** — toggle mark; **Space** — mark and, for folders, background size calc; **\*** — invert; **Ctrl+A** — mark all.
- Inline marks/size updates without full list rebuild.
- Panel **footer**: `panel.footer_ready` / `panel.footer_selected` (folders, files, total size).
- `entry_size_label_with_calculated` — show calculated directory size.
- **Quick-search** by typed characters (~1.2 s timeout), status `status.quick_search`.
- Lazy icons (`ensure_icon` / `reload_icons`); Symbolic / System style in settings.

---

## 14. UI — sorting and hidden files

**Files:** `main_window_actions.rs`, `file_panel_view.rs`, `columns.rs`, `shortcuts.rs`, View menu

- `Ctrl+F3..F6` — name / extension / date / size; column headers stay in sync.
- Remember / forget sort for a local folder.
- `Ctrl+.` — show hidden/system; stateful menu action.
- `Ctrl+R` — refresh (bypass SFTP cache); `Ctrl+\` — location root; `Ctrl+U` — swap panels.

---

## 15. UI — command bar (F-keys)

**Files:** `main_window_command_bar.rs`, `main_window.rs`, `theme_*.css`

- Buttons with separate **key** / **caption** (`.command-key`, `.command-caption`).
- Modifier modes: Normal / Alt / Ctrl / Shift / Hidden — captions update live.
- Alt: drives F1/F2; Ctrl: sort F3–F6; Shift: New File / Copy As / Wipe.
- Old navigation overlay removed from CSS.

---

## 16. UI — editor

**File:** `src/ui/editor_dialog.rs` (+~1000 lines)

- Independent window (`build_independent_window`) with File/Edit menu.
- Find / Replace / Go to line / Duplicate; shortcuts Ctrl+S/Shift+S, Ctrl+F/H/G, F3/Shift+F3, etc.
- **Remote edit:** download → edit → upload (`edit_remote_file`, `RemoteEditBinding`, progress).
- Size limit (`settings.max_edit_size_mb`); SourceView schemes for light/dark themes.
- **nginx** highlighting via bundled `nginx.lang`.

---

## 17. UI — operation, remote, base, settings dialogs

| Area | Changes |
|------|---------|
| `dialogs_operations.rs` | Editable destination path (copy/move/remote); separate **permanent delete** confirmation |
| `dialogs_remote.rs` | Full form i18n; save profile; **confirm_ssh_host_key** (fingerprint → known_hosts) |
| `dialogs_base.rs` | Independent windows, standard window buttons, `show_about`, `present_with_focus` |
| `dialogs_settings.rs` | Icon style; Editor section + max edit size MB |
| `dialogs.rs` | Removed `prompt_archive_open_action`; added `prompt_new_file`, `prompt_copy_as`; re-export new dialogs |
| `dialogs_unix.rs` | Enter activates default in chmod/chown |

---

## 18. UI — archives in the interface

**Files:** `main_window_navigation.rs`, `main_window_operations_archive.rs`, `dialogs.rs`

- Removed “open as archive / externally” choice; support via `ArchiveSupport`.
- On Windows **`.7z` is not opened via shell association** in-process — opened as an archive immediately.
- `archive_unsupported_*` dialog for unsupported formats.
- `main.rs` / `lib.rs`: `archive_worker_main()` entry for the worker process.

---

## 19. UI — file actions and navigation

**Files:** `main_window_actions_file.rs`, `main_window_navigation.rs`, `main_window_operations_transfer.rs`, `main_window_navigation_load.rs`, `main_window_navigation_watch.rs`, etc.

- **F2** — inline rename in the Name column.
- **Shift+F4** — new text file; **Shift+F5** — copy as; **Shift+F8 / Shift+Delete** — wipe.
- **Delete** mirrors F8 (recycle); Left/Right — parent / enter dir.
- Remote transfer with progress hooks; SFTP listing cache invalidation after ops.
- Busy state is **per panel**, not global; row size + footer updates.
- Panel splitter snap to center (18/32 px hysteresis) in `commander_view.rs`.

---

## 20. Themes / CSS

**Files:** `theme.rs`, `theme_base.css`, `theme_dark.css`, `theme_light.css`

- `gtk_application_prefer_dark_theme` for Light/Dark; `sourceview_style_scheme_ids`.
- Styles: tabs, footer, marked, menubar, command bar, help-hint; thinner paned separator.
- Navigation-overlay removed.

---

## 21. Shortcuts (new / extended)

**File:** `src/ui/shortcuts.rs`

Added/extended:
- Properties (`Alt+Enter`)
- Drives `Alt+F1` / `Alt+F2`
- `Shift+F4` / `Shift+F5` / `Shift+F8`
- `Delete` / `Shift+Delete`
- Sort `Ctrl+F3`–`Ctrl+F6`
- Favorites `Ctrl+D`
- Search `Alt+F7` / `Ctrl+Shift+F`
- Refresh `Ctrl+R`, root `Ctrl+\`, hidden `Ctrl+.`, swap panels `Ctrl+U`
- Mark all `Ctrl+A`
- Tabs `Ctrl+T` / `Ctrl+W` / `Ctrl+Tab` / …
- About; stateful toggle-hidden

---

## 22. Localization

### Russian — `locales/ru.yml` (**new**)
Full parallel translation of sections: `panel`, `column`, `entry`, `operation`, `common`, `command`, `menu`, `context`, `settings`, `dialog`, `search`, `editor`, `viewer`, `progress`, `terminal`, `archive`, `status`, `error`.

### `src/i18n.rs`
- `SUPPORTED_LOCALES`: `de`, `en`, `fr`, **`ru`**.
- Display name: **«Русский»**.

### `locales/{en,de,fr}.yml` (+~234 lines each)
New/changed keys: footer, menu.*, search.*, properties, favorites (+ move up/down), SSH host key, remote form, permanent delete, editor menu/find, folder sort, quick search, archive unsupported, copy/edit/favorites errors, etc.  
Removed/replaced: old archive-open dialog, old detail copy/move with fixed target, some archive status strings.

Favorites reorder keys:

| Key | en | ru | de | fr |
|-----|----|----|----|----|
| `dialog.favorites_move_up` | Move up | Вверх | Nach oben | Monter |
| `dialog.favorites_move_down` | Move down | Вниз | Nach unten | Descendre |

---

## 23. Presentation / entrypoints

| File | Change |
|------|--------|
| `presentation.rs` | `status_line` = raw status; new `panel_footer_line`; size with calculated dir size |
| `main.rs` | Early exit into archive worker |
| `lib.rs` | `pub use archive::archive_worker_main` |
| `gtk_app.rs` | Windows: `NON_UNIQUE`; apply `icon_style` at startup |

---

## 24. README.md

- Status: tabs, favorites, find-files, archives, properties, remote edit.
- Large shortcuts table (panels, F-keys, sort, marks, tabs, editor).
- Archives: ZIP, **7z**, RAR, ISO, libarchive; SFTP section (cache, F7, host trust).
- Dependencies: `sevenz-rust2`; list → `gtk::SingleSelection`.
- Roadmap clarified (remote move/rename/delete, etc.).

---

## 25. Unit tests (`tests/unit/**`)

### New
| File | Coverage |
|------|----------|
| `archive_sevenz_tests.rs` | detect / list+extract roundtrip |
| `domain_folder_sort_tests.rs` | normalize/upsert/session override |
| `domain_panel_tabs_tests.rs` | tabs |

### Modified
- `archive_detector_tests.rs` — `.7z`; extension wins over wrong content
- `domain_panel_tests.rs` / `domain_selection_tests.rs` — marks vs cursor, parent focus, mark_all/toggle
- `domain_sorting_tests.rs` — Type by extension
- `fs_reader_tests.rs` — `directory_total_size`, symlinks, Windows `.lnk` / junction
- `config_tests.rs` — editor, favorites, cursors, icon_style
- `remote_service_tests.rs` — known_hosts entry name, key labels
- `application_commander_tests.rs` / `application_load_scheduler_tests.rs` — adapted for `PanelTabs` / `tab_id` / sort in request
- `platform_icons_tests.rs` — `IconStyle` switch
- `i18n_tests.rs` — `ru` support

### Removed
- `platform_icons_windows_tests.rs`

---

## 26. Nginx language-spec

**New:** `assets/gtksourceview-5/language-specs/nginx.lang`

- GtkSourceView language `nginx`: comments, strings, `$variables`, directives/blocks, numbers with k/m/g.
- Globs: `nginx.conf`, `*.nginx`; editor also heuristically recognizes similar `.conf` files.

---

## Full list of affected paths

### Modified
```
Cargo.lock
Cargo.toml
README.md
locales/de.yml
locales/en.yml
locales/fr.yml
src/application/app_state.rs
src/application/commander.rs
src/application/commands.rs
src/application/errors.rs
src/application/mod.rs
src/application/navigation.rs
src/application/operation.rs
src/application/operation_runner.rs
src/application/services/entry_loader.rs
src/archive/detector.rs
src/archive/mod.rs
src/archive/path.rs
src/archive/probe.rs
src/archive/registry.rs
src/archive/service.rs
src/archive/unrar.rs
src/archive/zip_backend.rs
src/config.rs
src/domain/mod.rs
src/domain/panel.rs
src/domain/panel_location.rs
src/domain/selection.rs
src/domain/sorting.rs
src/fs/mod.rs
src/fs/operations.rs
src/fs/reader.rs
src/i18n.rs
src/lib.rs
src/main.rs
src/platform/icons.rs
src/platform/mod.rs
src/platform/windows.rs
src/presentation.rs
src/remote/mod.rs
src/remote/service.rs
src/ui/columns.rs
src/ui/commander_view.rs
src/ui/dialogs.rs
src/ui/dialogs_base.rs
src/ui/dialogs_operations.rs
src/ui/dialogs_remote.rs
src/ui/dialogs_settings.rs
src/ui/dialogs_unix.rs
src/ui/editor_dialog.rs
src/ui/file_panel_view.rs
src/ui/file_row_object.rs
src/ui/gtk_app.rs
src/ui/main_window.rs
src/ui/main_window_actions.rs
src/ui/main_window_actions_file.rs
src/ui/main_window_actions_remote.rs
src/ui/main_window_command_bar.rs
src/ui/main_window_context_menu.rs
src/ui/main_window_controllers.rs
src/ui/main_window_hosts.rs
src/ui/main_window_hosts_impl.rs
src/ui/main_window_navigation.rs
src/ui/main_window_navigation_load.rs
src/ui/main_window_navigation_watch.rs
src/ui/main_window_operations.rs
src/ui/main_window_operations_archive.rs
src/ui/main_window_operations_transfer.rs
src/ui/main_window_panel_wiring.rs
src/ui/main_window_window_chrome.rs
src/ui/main_window_window_state.rs
src/ui/shortcuts.rs
src/ui/theme.rs
src/ui/theme_base.css
src/ui/theme_dark.css
src/ui/theme_light.css
tests/unit/application_commander_tests.rs
tests/unit/application_load_scheduler_tests.rs
tests/unit/archive_detector_tests.rs
tests/unit/config_tests.rs
tests/unit/domain_panel_tests.rs
tests/unit/domain_selection_tests.rs
tests/unit/domain_sorting_tests.rs
tests/unit/fs_reader_tests.rs
tests/unit/i18n_tests.rs
tests/unit/platform_icons_tests.rs
tests/unit/remote_service_tests.rs
```

### Added (untracked)
```
docs/CHANGES-all-ru.md
docs/CHANGES-all-en.md
assets/gtksourceview-5/language-specs/nginx.lang
locales/ru.yml
src/archive/sevenz_backend.rs
src/archive/sevenz_worker.rs
src/archive/zip_name.rs
src/domain/folder_sort.rs
src/domain/panel_tabs.rs
src/domain/search.rs
src/fs/links.rs
src/fs/search.rs
src/fs/temp.rs
src/ui/dialogs_favorites.rs
src/ui/dialogs_properties.rs
src/ui/dialogs_search.rs
src/ui/main_window_menu.rs
tests/unit/archive_sevenz_tests.rs
tests/unit/domain_folder_sort_tests.rs
tests/unit/domain_panel_tabs_tests.rs
```

### Deleted
```
tests/unit/platform_icons_windows_tests.rs
```

### Not part of product changes
- Build directory `target-fix-7z/` and other `target*` — build artifacts, not source.

---

## Intentionally out of scope / limitations

- Favorites drag-and-drop reordering — not implemented (Move up/down buttons only).
- Find files — local directories only (not SFTP/archives).
- Remote move/rename/delete — roadmap, not in this change set.
- Favorites storage format (`FavoriteEntry`) — schema unchanged; order = `Vec` order.
