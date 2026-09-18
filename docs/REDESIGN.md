# Lilo interface redesign

Implementation and validation record, updated 17 September 2026.

## Implemented

| Area | Integrated behavior |
| --- | --- |
| Shared design | Graphite/lavender dark theme, warm paper light theme, system theme, density, typography, flat navigation, drawn icons, focus and disabled states. |
| Workspace | Resizable explorer, live Markdown editor, collapsible Outline/Links/Properties inspector. Inspector collapses below 1100 px; explorer becomes a drawer below 600 px. |
| Widget | Compact header, search, creation, pinning, workspace expansion and overflow actions. Daily navigation appears only for daily notes. Editor and per-note cursor/scroll state are shared with workspace mode. |
| Editor | Existing Markdown editing, tasks, formatting, wiki links, aliases, attachments, inline images, outline navigation and configurable toolbar placement remain connected. Readable width and text size are configurable. |
| Capture | Focused multiline input; Today, Inbox, New note and exact existing-note selection. Ctrl+Enter saves and Escape closes. A failed write retains the draft; successful capture preserves the active note. |
| Search | All, Notes, Tags, Commands and Saved scopes; structured filters, aliases, title/path/excerpts, Unicode match highlighting, Up/Down/Enter/Escape navigation. Results are cached and only visible rows are rendered. |
| Graph | Shared workspace/context graph model, local/folder/vault scopes, filters, dragging, zoom, fit and full view. Selection and related connections are emphasized. |
| Settings | Appearance, Editor, Daily Notes, Templates, Files & Storage, Attachments, Hotkeys, Privacy and About. Existing configuration and actions are preserved; storage commands open the relevant category. |
| Storage states | Explicit saved/pending/failed/conflict status, immediate pre-save external modification check, reload/keep-local conflict choices, dirty-buffer preservation when other files change, and a close guard for unsaved content. |
| Recovery | Existing backups, trash, restoration, diagnostics and vault operations remain available from navigation, menus and commands. |
| Performance | Cached graph selection, navigation tree and outgoing-link lookup, cached search results, background external-change snapshots, cached image references and asynchronous image loading. Tag metadata updates refresh the explorer immediately. |

## Main code locations

- `src/ui_style.rs`: theme tokens and common components.
- `src/app/navigation.rs`: workspace header, responsive panel toggles, shared navigation cache.
- `src/app/editor.rs`: editor integration.
- `src/app/settings.rs`: categorized settings and template actions.
- `src/commands.rs`, `src/quick_capture.rs`, `src/graph.rs`: integrated feature views.
- `src/note_preview.rs`, `src/vault_watch.rs`: preview caching and background snapshot worker.
- `src/app.rs`, `src/storage.rs`: state, persistence, conflict handling and action routing.

## Validation

- Windows: formatting, 118 tests, Clippy with warnings denied and the executable build all pass.
- Automated egui coverage renders editor, notes, graph, every settings category, capture and palette in both themes at widths 360, 760 and 1440 px. Compact coverage uses a 360 × 520 viewport and checks that selected note/content survive transitions.
- Regression coverage includes real capture writes, readonly destination failure and retry, exact existing-note destination, external changes before a watcher poll, focused palette navigation, Cyrillic glyphs and UTF-8-safe checkbox interaction.
- The current product screenshots cover Quick Capture, the compact editor, hierarchical tags and the expanded workspace: `assets/Screen1.png` through `assets/Screen4.png`.
- Linux remains covered by the cross-platform CI workflow; native window-manager behavior can differ between desktop environments.

## Deliberate scope

The system-wide capture hotkey remains Windows-only. Multi-tab and secondary-note-window workflows are not advertised because Lilo does not implement them.

`LILO_DATA_DIR` accepts an absolute directory for an isolated/portable instance. It holds settings, window state, cache and the default `Vault` directory. Portable mode does not register or remove OS autostart entries. Ordinary startup without this variable retains the existing storage locations.

Disposable UI-review data stays outside production defaults. The checked-in screenshots contain only the dedicated English showcase vault.
