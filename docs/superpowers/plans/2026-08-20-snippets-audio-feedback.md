# OpenDictate — Voice Snippets + Audio Feedback Implementation Plan

Status: Approved (2026-08-20)
Spec: `docs/superpowers/specs/2026-08-20-snippets-audio-feedback-design.md`

## Phase 1 — Core backend

1. `crates/opendictate-core/src/text.rs` — `fuzzy_match_trigger(spoken, triggers) -> Option<(String, f32)>`:
   lowercase token sets via `tokenize`, Dice coefficient on token bigrams, threshold ~0.6. Unit tests.
2. `src-tauri/src/db.rs` — `snippets` table migration in `open()`; `list_snippets`,
   `add_snippet`, `update_snippet`, `remove_snippet`, `import_snippets`, `export_snippets`. DB tests.
3. `src-tauri/src/state.rs` — `SnippetEntry` struct.
4. `src-tauri/src/dictation.rs` — `try_snippet_command(app, state, text)`; wire into
   `process_utterance` (before `commit_text`) and both streaming commit sites. Skip history on fire.
5. `src-tauri/src/commands.rs` — six snippet commands; register in `lib.rs`.

## Phase 2 — Snippets UI

6. `src/lib/api.ts` — `SnippetEntry`, `listSnippets`, `addSnippet`, `updateSnippet`,
   `removeSnippet`, `importSnippets`, `exportSnippets`.
7. `src/lib/store.ts` — `snippets` state, `setSnippets`, refresh in `refreshAll`.
8. `src/components/tabs/SnippetsTab.tsx` — table, add/edit/delete, quick capture, import/export.
9. `src/App.tsx` — register Snippets tab.

## Phase 3 — Audio feedback

10. `src-tauri/Cargo.toml` — add `rodio = "0.20"`.
11. `src-tauri/src/audio.rs` — `play_event(volume, event)`; three synthesized tones on a spawned thread.
12. `src-tauri/src/state.rs` + `commands.rs` — `audio_feedback`, `audio_feedback_volume` settings.
13. `dictation.rs` hooks — Listening on start, Inserted on success, Error on failure paths.

## Phase 4 — Settings UI

14. `src/lib/api.ts` — `audio_feedback`, `audio_feedback_volume` in `Settings` + `SettingsPatch`.
15. `src/components/tabs/GeneralTab.tsx` — audio toggle + volume slider (persistToggle pattern).

## Verification

- `cargo test -p opendictate-core --lib`
- `cargo test -p opendictate --lib`
- `cargo clippy --lib --all-features -- -D warnings`
- `npm run build`
- `touch src-tauri/src/lib.rs && cargo build --release`
- Manual: add snippet → dictate `insert snippet <name>` (inject + undo + no history);
  garbled name → error overlay; audio toggle + volume behavior.