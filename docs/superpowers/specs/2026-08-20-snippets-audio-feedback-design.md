# OpenDictate — Voice Snippets + Audio Feedback Design

Date: 2026-08-20
Status: Approved
Scope: dictation power tools (snippets) + accessibility (audio feedback)

## 1. Feature A — Voice-triggered plain-text snippets

Speak `insert snippet <name>` and OpenDictate expands a user-defined template into
the active window. Always-on (independent of the spoken-punctuation toggle).

### 1.1 Voice trigger grammar

- Case-insensitive detection of the phrase `insert snippet <name>`.
- **Snippet triggers are restricted to a single word** (validated on add/update/import).
  Only the first word after the prefix is treated as the name; any remaining
  words are dictated normally after the snippet text is inserted. This lets
  sentences continue after the expansion, e.g.
  `insert snippet signature and then call me` → insert "Best regards", then
  dictate "and then call me".
- Name is matched against the single-word triggers with best-effort fuzzy
  matching (token-alignment edit distance with per-token character similarity).
- Match above a confidence threshold (~0.6) → insert; below → red error dock state
  `Snippet not found: "<name>"` + notification.
- Only final commits act on the command (offline utterance + streaming endpoint);
  streaming partials never trigger insertion.
- The command words themselves are never injected.
- On match: the snippet text (plus any dictated tail) is injected via the existing
  injection pipeline, recorded in `last_inserted` so Undo works, `inserted` overlay
  feedback, no history entry.

### 1.2 Data model

New `snippets` table:

```
snippets (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  trigger    TEXT NOT NULL UNIQUE COLLATE NOCASE,
  text       TEXT NOT NULL,
  created_at TEXT NOT NULL
)
```

`SnippetEntry { id, trigger, text, created_at }` in state.rs.

### 1.3 Management UI — new Snippets tab

- Table of trigger + text with inline add/edit/delete (mirrors DictionaryTab).
- Quick capture: "Add from last dictation" prefills from `store.lastResult`.
- Export: JSON written to the exports dir (mirrors `export_history`); reveal in
  file manager. Import: hidden file input → JSON contents sent to backend, which
  validates and inserts, skipping triggers that already exist.

### 1.4 Commands

`list_snippets`, `add_snippet`, `update_snippet`, `remove_snippet`,
`import_snippets(contents)`, `export_snippets`.

## 2. Feature B — Audio feedback (synthesized, one toggle + volume)

Opt-in audio cues for dictation state. Default OFF.

### 2.1 Settings

- `audio_feedback: bool` — master toggle (default false).
- `audio_feedback_volume: f32` — 0.0–1.0 amplitude (default 0.5).

### 2.2 Sound set (synthesized via rodio, no bundled assets)

| Event     | When                     | Tone                                  |
|-----------|--------------------------|---------------------------------------|
| Listening | recording starts         | short rising two-tone                 |
| Inserted  | text/snippet inserted    | single high chime                     |
| Error     | mic/no-speech/paste fail | low descending tone                   |

Transcribing is omitted (sub-second, not worth a sound). Tones are generated on a
`rodio::OutputStream`/`Sink` in a spawned thread; amplitude scaled by volume;
failures are logged, never surfaced.

### 2.3 Playback hooks

- `Listening` — `start_recording`.
- `Inserted` — successful `commit_text` and snippet insert.
- `Error` — existing error paths (mic failure, no speech, failed paste).

## 3. Scope guardrails

- No new deps beyond `rodio` in the tauri crate.
- No streaming-partial snippet handling.
- No history entries for snippet expansions.
- `audio_feedback_volume` validated 0.0–1.0 in `set_settings`.