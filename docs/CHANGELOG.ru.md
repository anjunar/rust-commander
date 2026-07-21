# Все изменения проекта (незакоммиченные)

Дата сводки: 2026-07-21  
База сравнения: `master` @ `34e471a` (`edit: design`)  
Объём: **~87 изменённых tracked-файлов**, **+9031 / −1458** строк (без `Cargo.lock` и `target*`), плюс **~18 новых исходных/ресурсных файлов**.

Документ перечисляет **все** изменения рабочей копии относительно последнего коммита.  
Английская версия: [`CHANGES-all-en.md`](CHANGES-all-en.md).

---

## Сводка по темам

| Тема | Суть |
|------|------|
| **7z** | Новый backend (`sevenz-rust2`) + out-of-process worker на Windows |
| **ZIP names** | Unicode Path + эвристика CP866/CP1251 |
| **Вкладки** | Domain + AppState + UI + persist в конфиге |
| **Метки / курсор** | Классическая модель Total Commander (курсор ≠ marks) |
| **Folder sort** | Запоминание сортировки по папкам |
| **Поиск файлов** | Domain API + FS worker + диалог Alt+F7 |
| **Избранное** | Диалог, Ctrl+D, порядок Вверх/Вниз, компактный список |
| **Свойства** | Windows Explorer / встроенный диалог / SFTP chmod |
| **Меню** | Files / Commands / View / Help |
| **Редактор** | Независимое окно, Find/Replace, remote edit, nginx.lang |
| **SSH** | Trust host key, listing cache, ssh-agent fallback |
| **Симлинки / .lnk** | Классификация и безопасная навигация |
| **Windows safety** | Не открывать `.7z` в UI-процессе; icons/open out-of-process |
| **Русская локаль** | Новый `locales/ru.yml` |
| **Темы / command bar** | Вкладки, footer, marked, F-кнопки с модификаторами |
| **README** | Актуализация статуса, shortcuts, архивов, SFTP |

---

## Зависимости (`Cargo.toml` / `Cargo.lock`)

Добавлено:
- `sevenz-rust2 = "0.20"` — поддержка 7-Zip
- `encoding_rs = "0.8"` — декодировка имён ZIP
- `regex = "1.11"` — поиск файлов (regex)

Windows:
- у crate `windows` добавлен feature `Win32_Storage_FileSystem`

`Cargo.lock` обновлён соответственно.

---

## Новые файлы

### Исходники
| Файл | Назначение |
|------|------------|
| `src/archive/sevenz_backend.rs` | Backend list/extract для `.7z` |
| `src/archive/sevenz_worker.rs` | CLI-worker процесса (Windows OOP) |
| `src/archive/zip_name.rs` | Декодировка имён ZIP |
| `src/domain/panel_tabs.rs` | Модель вкладок панели |
| `src/domain/folder_sort.rs` | Сортировка, привязанная к папке |
| `src/domain/search.rs` | Модель find-files (glob/regex) |
| `src/fs/links.rs` | Symlink / `.lnk` / junction navigation |
| `src/fs/search.rs` | Фоновый cancellable walk поиска |
| `src/fs/temp.rs` | Temp-dir для remote edit |
| `src/ui/dialogs_favorites.rs` | Диалог избранного |
| `src/ui/dialogs_properties.rs` | Диалог свойств (локально + SFTP) |
| `src/ui/dialogs_search.rs` | Диалог поиска файлов |
| `src/ui/main_window_menu.rs` | Меню приложения |

### Локали / ресурсы / тесты / docs
| Файл | Назначение |
|------|------------|
| `locales/ru.yml` | Полная русская локаль |
| `assets/gtksourceview-5/language-specs/nginx.lang` | Подсветка nginx в редакторе |
| `tests/unit/archive_sevenz_tests.rs` | Тесты 7z |
| `tests/unit/domain_folder_sort_tests.rs` | Тесты folder sort |
| `tests/unit/domain_panel_tabs_tests.rs` | Тесты вкладок |
| `docs/CHANGES-all-ru.md` | Этот сводный файл (RU) |
| `docs/CHANGES-all-en.md` | Сводный файл (EN) |

### Удалено
| Файл | Причина |
|------|---------|
| `tests/unit/platform_icons_windows_tests.rs` | Убран вложенный include; сценарии перенесены/упрощены |

---

## 1. Архивы (`src/archive/**`)

### 1.1. Поддержка 7-Zip
- Формат `ArchiveFormat::SevenZip` / семейство `ArchiveFamily::SevenZip`.
- `SevenZipBackend` — list/extract, маппинг ошибок (пароль, CRC, I/O).
- На **Windows** list/extract выполняются в **отдельном процессе** (`--archive-worker`), чтобы не грузить shell-extension 7-Zip в UI (обход `STATUS_STACK_BUFFER_OVERRUN`).
- `sevenz_worker.rs` — протокол stdout с разделителем `\x1f` (`list` / `extract`).
- `archive_worker_main()` в `archive/mod.rs`; ранний выход из `main.rs` / реэкспорт в `lib.rs` без GTK.

### 1.2. Имена ZIP
- `zip_name.rs`: Info-ZIP Unicode Path (0x7075) → UTF-8 → эвристика CP866/CP1251 → fallback crate.
- Использует `encoding_rs`.

### 1.3. Детекция и registry
- Детекция: **сначала расширение**, потом magic bytes; известный суффикс (в т.ч. `.7z`) **не открывает файл** только ради detection.
- `is_archive_path` / Enter: extension-first — `.7z` на Windows не уходит в `read_dir`/shell.
- `AskArchiveAction` → **`OpenArchive`** (открытие архива напрямую, без выбора «как архив / внешне»).
- `resolve_for_path` → `backend_that_can_open`; отдельно `probe_path()` / `can_open_path()` для UX «ещё не поддерживается».
- Multipart ZIP probe: суффикс по **байтам**, не по char-slice (фикс panic на кириллических именах).
- Общие хелперы: `normalize_archive_entry_path`, `archive_entry_display_name`, `archive_entry_matches_selection`.
- Zip/Unrar переведены на общие path-хелперы; extract одного entry → через `extract_entries`.

### Изменённые файлы
`detector.rs`, `mod.rs`, `path.rs`, `probe.rs`, `registry.rs`, `service.rs`, `unrar.rs`, `zip_backend.rs`  
Новые: `sevenz_backend.rs`, `sevenz_worker.rs`, `zip_name.rs`

---

## 2. Domain (`src/domain/**`)

### 2.1. Вкладки — `panel_tabs.rs`
- `PanelTabs`, `TabId`
- open / close / activate / next / prev

### 2.2. Folder sort — `folder_sort.rs`
- Нормализация пути, lookup/upsert/remove
- Session override побеждает preference из конфига

### 2.3. Поиск — `search.rs`
- `SearchOptions`, `SearchHit`, `CompiledSearch`
- glob (`*` / `?`) и regex, exclude-маски

### 2.4. Panel / selection / sorting / location
- Курсор ≠ метки: `SelectionModel::single` больше не помечает строку; метки независимы.
- `selected_items()`: если нет меток — берёт **курсор** (классический commander).
- Mark API: `toggle_mark`, `toggle_mark_in_place`, `mark_all`, `invert_marks`, `set_cursor`, `set_marks`.
- `replace_entries` / `navigate_to` **не сортируют** — сортировка на worker.
- При navigate учитывается `entry_key_when_leaving_to` (фокус на покинутой папке).
- Расчёт размера каталогов: `calculated_dir_sizes`, `set_calculated_dir_size`.
- Quick-search: `index_matching_name_prefix`.
- `PanelLocation`: `tab_title()`, `root()`, `entry_key_when_leaving_to()`; `parent()` для FS у корня → `None`.
- Сортировка **Type** — по **расширению файла**, не по `EntryKind`.
- `SortColumn` / `SortDirection`: `Serialize`/`Deserialize`, `Default`.
- `folders_first` сделан `pub`.

### Изменённые / новые
Изменены: `mod.rs`, `panel.rs`, `panel_location.rs`, `selection.rs`, `sorting.rs`  
Новые: `panel_tabs.rs`, `folder_sort.rs`, `search.rs`

---

## 3. Файловая система (`src/fs/**`)

### Добавлено
- **`links.rs`**: symlink / Windows `.lnk`; `resolve_directory_navigation_path` (junction fallback, напр. `Documents and Settings` → `Users`).
- **`search.rs`**: фоновый cancellable walk → `SearchEvent` (Progress / Hits / Failed / Finished).
- **`temp.rs`**: temp-dir для remote edit.
- `directory_total_size()` — рекурсивная сумма размеров (не заходит во вложенные symlink-dirs).
- `copy_entry()` — рекурсивное копирование дерева.
- Single-source transfer: опциональный явный `target_path`.

### Изменено
- Listing через `classify_fs_entry`; `read_entry` → `symlink_metadata`.
- Атрибуты: флаг `L` для ссылок; на Unix symlink определяется раньше directory.
- Навигация в «папку» резолвит path через `resolve_directory_navigation_path`.

### Файлы
Изменены: `mod.rs`, `operations.rs`, `reader.rs`  
Новые: `links.rs`, `search.rs`, `temp.rs`

---

## 4. Application (`src/application/**`)

### Добавлено
- Вкладки: `open_tab` / `close_tab` / `activate_tab` / `next_tab` / `prev_tab` / `restore_tabs` / `swap_panels`.
- Marks + quick search: `set_cursor`, `toggle_mark*`, `mark_all`, `invert_marks`, `quick_search`, `set_calculated_dir_size`.
- `CreateDirectoryTarget` — FS **или** Remote.
- `create_file_request`, `copy_with_rename_request`.
- Навигация: `parent_navigation_request`, `filesystem_navigation_request`, `location_root_navigation_request`.
- `DeleteMode` (`Default` | `Permanent`); `prepare_operation_with_options`.
- `NavigationError::HostKeyTrustRequired`.

### Изменено
- `AppState.left/right`: `Panel` → **`PanelTabs`**; `panel()` = активная вкладка.
- `NavigationRequest` / `DirectoryLoadResult`: `tab_id`, `sort_state`, `folders_first`; sort на worker.
- Архивы: `AskArchiveAction` → `OpenArchive`; проверка архива **до** `is_dir`.
- `ViewUpdate::selection`: обновляет entries панели **без** смены `active_panel` (меньше дёрганья скролла).
- После Copy: сразу clear marks; Delete/Move — queue delete selection.
- Remote ops: `force_overwrite`; Local: `target_path`.
- `EntryLoader`: `force_refresh` + `session_key`; маппинг untrusted host key.

### Файлы
`app_state.rs`, `commander.rs`, `commands.rs`, `errors.rs`, `mod.rs`, `navigation.rs`, `operation.rs`, `operation_runner.rs`, `services/entry_loader.rs`

---

## 5. Конфиг (`src/config.rs`)

Добавлено:
- `favorites: Vec<FavoriteEntry>`
- `folder_sort: Vec<FolderSortPreference>`
- `EditorConfig` (размер окна, max edit size MiB)
- `IconStyle` (`Symbolic` | `System`) в `GeneralConfig`
- `PaneTabConfig` + `left_tabs` / `right_tabs` / `*_active_tab`
- Legacy-поля: `left_cursor` / `right_cursor`

Поведение: старые конфиги без новых полей грузятся с `Default`; вкладки/курсоры/избранное/folder_sort сериализуются в TOML.

---

## 6. Remote / SFTP (`src/remote/**`)

### Добавлено
- Интерактивный trust host key: `HostKeyTrustOffer`, `HostKeyUntrusted`, `RemoteService::trust_host_key`, запись в OpenSSH `known_hosts`.
- Кэш листингов SFTP (`listing_cache`); `invalidate_listing_cache_for_path`; `force_refresh` обходит кэш.
- `create_directory`, `set_permissions`.
- Symlink в remote listing (`EntryKind::Symlink`, follow для is_dir).
- `force_overwrite` для download/upload.
- Auth: fallback **ssh-agent** после неудачи pubkey file (важно для Ed25519 на Windows/WinCNG).

### Изменено
- Нет `known_hosts` / NotFound → `HostKeyUntrusted` (UI может предложить trust), а не жёсткий bail.
- `read_entries` принимает `session_key` + `force_refresh`.

### Файлы
`mod.rs`, `service.rs`

---

## 7. Platform (`src/platform/**`)

### Добавлено
- `IconStyle` runtime + `set_icon_style` / `icon_for_listing_entry` / `fallback_icon_name_for_row`.
- Windows: `show_file_properties`, `force_foreground_window`, `resolve_shortcut_target` (COM `.lnk`).
- `open_path` через **out-of-process** `cmd /C start` (не ShellExecute in-process).

### Изменено
- Иконки списка: только extension/directory hints + `USEFILEATTRIBUTES` — **не открывают** живые пути вроде `.7z`.
- Удалён `platform_icons_windows_tests.rs`.

### Файлы
`icons.rs`, `mod.rs`, `windows.rs`

---

## 8. UI — вкладки панелей

**Файлы:** `file_panel_view.rs`, `commander_view.rs`, `main_window.rs`, `main_window_actions.rs`, `main_window_panel_wiring.rs`, `main_window_window_state.rs`, `shortcuts.rs`, `theme_*.css`

- Полоса вкладок (`.panel-tab-bar` / `.panel-tab` / `.panel-tab-add`) + кнопка «+».
- Действия: новая / закрыть / след. / пред.
- Шорткаты: `Ctrl+T`, `Ctrl+W`, `Ctrl+Tab` / `Ctrl+PageDown`, `Ctrl+Shift+Tab` / `Ctrl+PageUp`.
- Сессия: сохранение/восстановление `left_tabs` / `right_tabs` и активного индекса (`persist_pane_session`).
- Сортировка и курсор — **на вкладку**.

---

## 9. UI — меню приложения

**Новый:** `src/ui/main_window_menu.rs`  
**Связано:** `main_window.rs`, `main_window_window_chrome.rs`, `shortcuts.rs`

`PopoverMenuBar`:

| Меню | Пункты |
|------|--------|
| **Файлы** | Открыть, Просмотр, Правка, Свойства, Переименовать; Новый файл / Каталог; Вкладки; Избранное / SFTP; Выход |
| **Команды** | Копировать / Копировать как / Переместить / Удалить / Стереть; Поиск; Выделить всё; Смена/обмен панелей; Терминал |
| **Вид** | Сортировки; Запомнить/забыть сортировку папки; Обновить; Скрытые файлы; Диски; Корень |
| **Справка** | Настройки, О программе |

`refresh_menu_bar` обновляет подписи при смене языка.

---

## 10. UI — поиск файлов

**Новый:** `src/ui/dialogs_search.rs`  
**Связано:** `dialogs.rs`, `main_window_actions.rs`, `shortcuts.rs`, ключи `search.*`

- Диалог Find files (`Alt+F7` / `Ctrl+Shift+F`): шаблон имени, exclude, каталог, подпапки, регистр, regex.
- Результаты в `ColumnView` (имя, тип, путь, размер, дата); Start/Stop; статусы.
- Только локальные каталоги; **GoTo** открывает папку результата и фокусирует файл.
- Экспорт: `show_find_files`, `SearchDialogAction`, `search_go_to_request`.

---

## 11. UI — свойства файлов

**Новый:** `src/ui/dialogs_properties.rs`  
**Связано:** `main_window_actions_file.rs`, `shortcuts.rs` (`Alt+Enter`)

- **Windows + локальный ФС:** нативный лист Explorer.
- **Linux/macOS / архивы:** встроенный диалог (имя, путь, тип, размер, даты, атрибуты).
- **SFTP:** `show_remote_properties` — octal + чекбоксы owner/group/other (rwx), Apply.

---

## 12. UI — избранное

**Новый модуль** `dialogs_favorites.rs` (подключён в `dialogs.rs`)

### Полная фича
- Список избранного; Add current / Edit / Delete / Go; диалог имя+путь.
- Хранение через `FavoritesAction::Update`; переход — `FavoritesAction::Go`.
- Горячая клавиша **`Ctrl+D`**; пункт меню.

### Доработка порядка и компактности
- Кнопки **Вверх / Вниз** (`favorites_move_up` / `favorites_move_down`) после «Удалить».
- `move_selected(delta)`: swap соседних, persist через `FavoritesAction::Update`, перенос фокуса.
- Компактные строки: **только имя**, путь в tooltip, меньшие отступы, без `dialog-title`.
- DnD не реализовывался; формат `FavoriteEntry` в конфиге без изменений.

---

## 13. UI — выделение, маркировка, footer, quick-search

**Файлы:** `file_panel_view.rs`, `columns.rs`, `file_row_object.rs`, `presentation.rs`, `commander_view.rs`, `main_window_panel_wiring.rs`, `main_window_hosts_impl.rs`

- Multi-selection → **курсор + marks** (`SingleSelection`); marked — красный `#ff4d4d` + CSS `.marked`.
- **Insert** — toggle mark; **Space** — mark и при пометке папки — фоновый расчёт размера; **\*** — invert; **Ctrl+A** — mark all.
- Inline-обновление marks/size без полной пересборки списка.
- **Footer** панели: `panel.footer_ready` / `panel.footer_selected` (папки, файлы, суммарный размер).
- `entry_size_label_with_calculated` — показ рассчитанного размера каталога.
- **Quick-search** по набору символов (таймаут ~1.2 с), статус `status.quick_search`.
- Ленивые иконки (`ensure_icon` / `reload_icons`); стиль Symbolic / System в настройках.

---

## 14. UI — сортировка и скрытые файлы

**Файлы:** `main_window_actions.rs`, `file_panel_view.rs`, `columns.rs`, `shortcuts.rs`, меню View

- `Ctrl+F3..F6` — имя / расширение / дата / размер; синхронизация заголовков колонок.
- Запомнить / забыть сортировку для локальной папки.
- `Ctrl+.` — показ скрытых/системных; stateful action в меню.
- `Ctrl+R` — обновление (обход SFTP cache); `Ctrl+\` — корень локации; `Ctrl+U` — обмен панелей.

---

## 15. UI — командная панель (F-ключи)

**Файлы:** `main_window_command_bar.rs`, `main_window.rs`, `theme_*.css`

- Кнопки с раздельными **key** / **caption** (`.command-key`, `.command-caption`).
- Режимы по модификаторам: Normal / Alt / Ctrl / Shift / Hidden — подписи меняются «на лету».
- Alt: диски F1/F2; Ctrl: сортировки F3–F6; Shift: New File / Copy As / Wipe.
- Убран старый navigation overlay из CSS.

---

## 16. UI — редактор

**Файл:** `src/ui/editor_dialog.rs` (+~1000 строк)

- Независимое окно (`build_independent_window`) с меню File/Edit.
- Find / Replace / Go to line / Duplicate; шорткаты Ctrl+S/Shift+S, Ctrl+F/H/G, F3/Shift+F3 и др.
- **Удалённое редактирование:** download → edit → upload (`edit_remote_file`, `RemoteEditBinding`, прогресс).
- Лимит размера (`settings.max_edit_size_mb`); схемы SourceView под светлую/тёмную тему.
- Подсветка **nginx** через bundled `nginx.lang`.

---

## 17. UI — диалоги операций, remote, base, settings

| Область | Изменения |
|---------|-----------|
| `dialogs_operations.rs` | Редактируемый путь назначения (copy/move/remote); отдельное подтверждение **permanent delete** |
| `dialogs_remote.rs` | Полная i18n формы; save profile; **confirm_ssh_host_key** (fingerprint → known_hosts) |
| `dialogs_base.rs` | Independent windows, стандартные кнопки окна, `show_about`, `present_with_focus` |
| `dialogs_settings.rs` | Стиль иконок; секция Editor + max edit size MB |
| `dialogs.rs` | Удалён `prompt_archive_open_action`; добавлены `prompt_new_file`, `prompt_copy_as`; реэкспорт новых диалогов |
| `dialogs_unix.rs` | Enter активирует default в chmod/chown |

---

## 18. UI — архивы в интерфейсе

**Файлы:** `main_window_navigation.rs`, `main_window_operations_archive.rs`, `dialogs.rs`

- Убран выбор «открыть как архив / внешне»; поддержка/неподдержка через `ArchiveSupport`.
- На Windows **`.7z` не открывается через shell association** в процессе — сразу как архив.
- Диалог `archive_unsupported_*` при неподдерживаемом формате.
- `main.rs` / `lib.rs`: вход `archive_worker_main()` для worker-процесса.

---

## 19. UI — файловые действия и навигация

**Файлы:** `main_window_actions_file.rs`, `main_window_navigation.rs`, `main_window_operations_transfer.rs`, `main_window_navigation_load.rs`, `main_window_navigation_watch.rs`, и др.

- **F2** — inline rename в колонке Name.
- **Shift+F4** — новый текстовый файл; **Shift+F5** — copy as; **Shift+F8 / Shift+Delete** — wipe.
- **Delete** дублирует F8 (recycle); Left/Right — parent / enter dir.
- Remote transfer с progress hooks; инвалидация SFTP listing cache после операций.
- Busy-состояние **на панель**, не глобально; обновление размера строки + footer.
- Snap разделителя панелей к центру (гистерезис 18/32 px) в `commander_view.rs`.

---

## 20. Темы / CSS

**Файлы:** `theme.rs`, `theme_base.css`, `theme_dark.css`, `theme_light.css`

- `gtk_application_prefer_dark_theme` для Light/Dark; `sourceview_style_scheme_ids`.
- Стили: вкладки, footer, marked, menubar, command bar, help-hint; уже разделитель paned.
- Убран navigation-overlay.

---

## 21. Горячие клавиши (новые / расширенные)

**Файл:** `src/ui/shortcuts.rs`

Добавлены/расширены:
- Свойства (`Alt+Enter`)
- Диски `Alt+F1` / `Alt+F2`
- `Shift+F4` / `Shift+F5` / `Shift+F8`
- `Delete` / `Shift+Delete`
- Сортировки `Ctrl+F3`–`Ctrl+F6`
- Избранное `Ctrl+D`
- Поиск `Alt+F7` / `Ctrl+Shift+F`
- Refresh `Ctrl+R`, корень `Ctrl+\`, скрытые `Ctrl+.`, обмен панелей `Ctrl+U`
- Mark all `Ctrl+A`
- Вкладки `Ctrl+T` / `Ctrl+W` / `Ctrl+Tab` / …
- About; stateful toggle-hidden

---

## 22. Локализация

### Русский — `locales/ru.yml` (**новый**)
Полный параллельный перевод секций: `panel`, `column`, `entry`, `operation`, `common`, `command`, `menu`, `context`, `settings`, `dialog`, `search`, `editor`, `viewer`, `progress`, `terminal`, `archive`, `status`, `error`.

### `src/i18n.rs`
- `SUPPORTED_LOCALES`: `de`, `en`, `fr`, **`ru`**.
- Отображаемое имя: **«Русский»**.

### `locales/{en,de,fr}.yml` (+~234 строки каждый)
Новые/изменённые ключи: footer, menu.*, search.*, properties, favorites (+ move up/down), SSH host key, remote form, permanent delete, editor menu/find, folder sort, quick search, archive unsupported, ошибки копирования/редактирования/избранного и др.  
Удалены/заменены: старый archive-open dialog, старые detail copy/move с фиксированным target, часть archive status strings.

Ключи избранного (reorder):

| Ключ | en | ru | de | fr |
|------|----|----|----|----|
| `dialog.favorites_move_up` | Move up | Вверх | Nach oben | Monter |
| `dialog.favorites_move_down` | Move down | Вниз | Nach unten | Descendre |

---

## 23. Presentation / entrypoints

| Файл | Изменение |
|------|-----------|
| `presentation.rs` | `status_line` = сырой статус; новый `panel_footer_line`; size с calculated dir size |
| `main.rs` | Ранний выход в archive worker |
| `lib.rs` | `pub use archive::archive_worker_main` |
| `gtk_app.rs` | Windows: `NON_UNIQUE`; применение `icon_style` при старте |

---

## 24. README.md

- Статус: вкладки, favorites, find-files, архивы, свойства, remote edit.
- Большая таблица shortcuts (панели, F-ключи, сортировка, marks, вкладки, редактор).
- Архивы: ZIP, **7z**, RAR, ISO, libarchive; секция SFTP (cache, F7, host trust).
- Зависимости: `sevenz-rust2`; список → `gtk::SingleSelection`.
- Roadmap уточнён (remote move/rename/delete и т.д.).

---

## 25. Unit-тесты (`tests/unit/**`)

### Новые
| Файл | Содержание |
|------|------------|
| `archive_sevenz_tests.rs` | detect / list+extract roundtrip |
| `domain_folder_sort_tests.rs` | normalize/upsert/session override |
| `domain_panel_tabs_tests.rs` | вкладки |

### Изменены
- `archive_detector_tests.rs` — `.7z`; extension побеждает неверный content
- `domain_panel_tests.rs` / `domain_selection_tests.rs` — marks vs cursor, parent focus, mark_all/toggle
- `domain_sorting_tests.rs` — Type по расширению
- `fs_reader_tests.rs` — `directory_total_size`, symlinks, Windows `.lnk` / junction
- `config_tests.rs` — editor, favorites, cursors, icon_style
- `remote_service_tests.rs` — known_hosts entry name, key labels
- `application_commander_tests.rs` / `application_load_scheduler_tests.rs` — адаптация под `PanelTabs` / `tab_id` / sort в request
- `platform_icons_tests.rs` — смена `IconStyle`
- `i18n_tests.rs` — поддержка `ru`

### Удалены
- `platform_icons_windows_tests.rs`

---

## 26. Nginx language-spec

**Новый:** `assets/gtksourceview-5/language-specs/nginx.lang`

- GtkSourceView language `nginx`: комментарии, строки, `$variables`, директивы/блоки, числа с k/m/g.
- Globs: `nginx.conf`, `*.nginx`; редактор также эвристически распознаёт похожие `.conf`.

---

## Полный список затронутых путей

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

### Не входит в изменения продукта
- Каталог сборки `target-fix-7z/` и прочие `target*` — артефакты компиляции, не исходники.

---

## Что намеренно не делалось / ограничения

- Drag-and-drop для перестановки избранного — нет (только кнопки Вверх/Вниз).
- Поиск файлов — только локальные каталоги (не SFTP/архивы).
- Remote move/rename/delete — в roadmap, не в этом наборе изменений.
- Формат хранения избранного (`FavoriteEntry`) — без изменений схемы; порядок = порядок `Vec`.
