# Performance guardrails

Lilo keeps the compact interface responsive with bounded work rather than loading an unlimited amount of visual detail every frame.

## Current safeguards

- Markdown highlighting is cached by content, active line, font size and theme.
- Notes above 512 KiB use a lightweight plain-text layout while remaining fully editable.
- Search uses a pre-normalised per-note text field.
- External file checks run on a two-second interval instead of every frame.
- Graph rendering has a user-adjustable 10–200 node limit and deterministic positions, so unrelated notes do not reshuffle existing nodes.
- Autosave writes only dirty notes at the configured 15-second to 10-minute interval; explicit save and application exit still flush pending changes.
- Multi-note saves refresh the vault snapshot once after the batch instead of rescanning after every note.
- Link and tag indexes refresh after a short typing pause instead of rebuilding on every keystroke.

## Automated workloads

The test suite includes regression workloads for:

- a 301-note vault distributed across nested folders, including malformed metadata;
- a 1000-note link index with a 120-node global graph selection and layout;
- a Markdown note larger than the live-highlighting threshold;
- Unicode selections, lists, backups, import and full-vault export.

The large-vault and dense-graph tests use a generous ten-second ceiling. This is a regression limit for varied development machines, not a claim that normal operation should take ten seconds. Run all checks with `cargo test`; use a release build for representative manual UI profiling.
