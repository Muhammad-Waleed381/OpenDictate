# Native integration, database, and packaging repairs

This report covers the parent's changes after the Finder → Skeptic → Arbiter reviews.
The audit base is `cda12cb`; severity and original scoring stay in those reports.

| Bug | Repair | Verification |
|---|---|---|
| B025 | Saved legacy Local SLM settings normalize to off before runtime application, allowing unrelated preferences to change without opting into cloud processing. | Actual legacy settings load/save regression reproduced red, then green. |
| B026 | macOS app names are `open -a` arguments; Windows app names are environment data consumed by fixed PowerShell, with a failed AppActivate returning an error. URLs use the pinned safe opener rather than `cmd /C start`. Sway criteria escape both regular-expression and command-string syntax. | Malicious-name command construction tests; pinned opener implementation inspected. Windows/macOS execution not performed. |
| B028 | A failed clipboard write returns an error before any paste key. Linux fallbacks require both successful stdin write and successful process exit, with bounded process waits. | All-failed/successful writer policy tests. Real compositor clipboard behavior still needs platform testing. |
| B029 | Explicit `input_target`: text fields use ordinary paste and line selection/deletion; Bash/Zsh shell profile uses terminal paste (Ctrl+Shift+V on Linux) and Ctrl+A then Ctrl+K for complete readline input deletion. Windows text deletion selects the full line instead of deleting one word. | Native generated-chord and settings validation tests; frontend selection/save regression. Custom shell/vi key maps are outside the supported profile. |
| B030 | All Linux fallbacks generate complete modifier-down, key-click, modifier-up sequences. ydotool receives explicit numeric key transitions. uinput errors propagate and modifiers are released even after a failed write. | wtype argument regressions for undo, multi-modifier and plain keys. |
| B031 | UI Insert/Test Insert/Undo run off the main thread, hide focused OpenDictate windows, then check that no app window remains focused. Failed handoff restores the UI and reports the error. Undo state is consumed only after success. | Focus rejection policy and failed-Undo retry regression. Actual window-manager restoration needs native desktop validation. |
| B041 | CSV rows now match the header: id, quoted text, timestamp, duration, quoted source. Text/source quotes and line breaks are escaped consistently. | Exact exported-row assertions; original implementation independently fails the same expectation. |
| B042 | Exports query all history, independent of the recent-history UI's 500-row limit. | Database fixture inserts 503 rows; UI returns 500, export returns all 503 including the oldest. |
| B043 | Reset stores the latest history ID transactionally. Old history edits/deletes do not change subsequent statistics. | Reset → new activity → edit/delete old history preserves totals; new activity still adjusts correctly. Original implementation reproduces a 3→0 corruption. |
| B044 | Inserts, adjustments and startup backfill count Unicode whitespace-separated words. Existing unreset aggregates migrate once. | Empty/repeated-space/tab/newline/Unicode whitespace tests. Original counting function fails. |
| B045 | Streak calculation treats zero-word today like absent today and can continue yesterday's streak. | Missing/zero/nonzero-today table regression. |
| B048 | Capability includes the actual `dock` window label. | Tauri build/check validates capability configuration; no live dock IPC test. |
| B052 | `test_snippet` reads the clipboard before replacement, expands the normal snippet template, and inserts via the focus handoff. Copy retains literal template behavior. | Frontend IPC regression and core template tests; no live keystrokes. |
| B055 | GNOME binding invokes the installed executable with `--toggle`; scripts resolve installed app/AppImage or `OPENDICTATE_BIN`. Single-instance and cold-start dispatch are integrated by the session worker. No developer paths or hardcoded data directories. | Source/call-path review, shell syntax checks, portable launcher fixture. |
| B056 | GNOME binding parsing handles `@as []`, preserves valid existing paths, and rejects malformed lists. gsettings failures propagate. | Typed-empty/normal/malformed list regressions. |
| B057 | macOS keeps the old double-tap gesture armed until new shortcut registration succeeds. | Control-flow review; actual macOS registration unavailable. |
| B058 | Hotkey callbacks enqueue synchronously to one FIFO worker; press/release no longer race to acquire a mutex on separate threads. | 100 queued requests retain order despite a deliberately delayed first handler. |
| B059 | Linux autostart and GNOME bindings use the stable `APPIMAGE` path when present, validate it, and correctly quote desktop Exec arguments. | AppImage transient-mount and regular-installed-path tests, invalid relative-path rejection. |
| B060 | LaunchAgent executable paths are XML escaped. | Ampersand, angle-bracket and quote fixture. |
| B061 | Experimental GPU job detects archive content, forwards shared-library features with CPU defaults disabled, packages runtime libraries with app and per-library rpaths, and uploads the workspace artifact path with missing artifacts treated as failure. Existing Debian/RPM configuration survives library-map injection. | Three local archive/config fixtures; exact Cargo feature tree shows sherpa `shared` without `static`; YAML and CLI arguments inspected. GPU installer/hardware build not run. |

## Review corrections and integration

An independent patch review identified that a provisional GTK `set_text` path
could not prove PRIMARY ownership, that replacing one global delete-line chord
with another left terminal behavior broken, and that copied/failed results could
enable Undo for an older insertion. The PRIMARY path was removed, explicit target
profiles were added, and frontend/native Undo eligibility now follows the latest
successful insertion. These are refinements of B027/B029/B031/B050, not extra
scored candidates.

`complete_onboarding` shares the settings-update gate with preference changes,
saves before publishing in-memory success, and runs on a blocking worker. Tray
Undo awaits the new async command and surfaces failures.

## Validation detail and limits

Three baseline functions were extracted verbatim from `cda12cb` into a temporary
Rust harness linked to the already-built SQLite crate. CSV order, whitespace
word counts, and post-reset old-history edits all failed their desired behavior
assertions. The matching repository regressions pass after repair. This avoided
reverting other workers' in-progress files. The packaging fixtures were run red
before implementation and green afterward.

Legacy databases that recorded a reset without its history boundary cannot
reconstruct which existing rows contributed to post-reset totals. Migration
preserves those totals and conservatively excludes existing history from future
adjustments; newly inserted rows are tracked normally. No history is deleted.

No real desktop text injection, platform autostart mutation, GPU download, or
production database mutation was performed. Final aggregate checks and platform
limits are recorded in `validation.md`.
