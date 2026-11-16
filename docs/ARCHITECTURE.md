# Lilo architecture

Lilo keeps user data as ordinary Markdown and treats application settings as a separate concern. `src/app.rs` owns `LiloApp`, its durable and transient state, construction, the `eframe::App` implementation and the root module list. Behavior lives in focused `src/app/` modules:

- `shell.rs` composes the viewport, responsive panels, overlays and keyboard routing;
- `actions.rs` executes shared `CommandAction` values from menus, compact controls, shortcuts and the command palette;
- `notes.rs` owns note creation, opening, navigation, rename and deletion workflows;
- `persistence.rs` coordinates atomic saves, autosave, conflicts, indexes and the background vault watcher;
- `explorer.rs` and `inspector.rs` render the current left and right panels;
- `recovery.rs` and `vault.rs` handle recovery views, import/export and vault switching;
- `editor.rs`, `navigation.rs` and `settings.rs` contain the editor, current header/navigation controls and settings UI;
- `find.rs`, `completion.rs` and `status.rs` add in-note find/replace, `[[link]]`/`#tag` autocompletion and writing statistics around the editor;
- `state.rs` groups workflow-specific transient state (recovery, history, folders, tags, analytics, find, completion) so `LiloApp` itself stays small.

UI-independent editor logic lives in `src/editor_tools.rs` (character-index based text statistics, find/replace and completion context) and is unit tested separately. The Ctrl+O quick switcher is `src/quick_switcher.rs`, next to the command palette in `src/commands.rs`. Large-vault benchmarks are in `src/benchmarks.rs`.

`src/storage.rs` is the storage facade. Its submodules separate settings compatibility, vault paths, Markdown note I/O, Trash and Backups, import/export, diagnostics and atomic writes. Existing callers continue to use `storage::...`, so the split does not spread module details through the application.

Application settings use an explicit compatibility boundary. Direct settings upgrades are supported from Lilo 0.2.0, whose settings format version is 6. Removed navigation fields in newer JSON files are ignored. Files with an explicit older version are preserved as invalid backups and replaced with current defaults; files without a trustworthy version are parsed using known fields and defaults. Damaged JSON or invalid UTF-8 is also backed up before defaults are used.

This boundary never applies to vault contents. Markdown notes, images, attachments and folders remain normal files and can be opened or copied without Lilo.
