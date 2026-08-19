# Spoken Punctuation Control — Design

**Date:** 2026-08-19
**Status:** Approved
**Scope:** Dictation reliability & accuracy (first phase of the three-area plan)

## Summary

Add a settings toggle, off by default, that converts the spoken words
"period", "comma", "question mark", and "exclamation point" into their
punctuation symbols in transcribed text. Existing smart auto-punctuation
(`clean_text`) is unchanged and composes with the new mapping.

## Behavior

When the toggle is enabled:

| Spoken | Inserted |
| ------ | -------- |
| "period" | `.` |
| "comma" | `,` |
| "question mark" | `?` |
| "exclamation point" (or "exclamation mark") | `!` |

- Matching is case-insensitive and token-based: a standalone word/phrase is
  replaced only when it appears as its own token, never inside another word
  ("periodontist" is untouched).
- The word "point" is NOT mapped, preserving decimals ("three point five").
- Mapping runs first in the pipeline, so a mapped `.?!` still triggers sentence
  capitalization and space-stripping, and punctuation words win over dictionary
  terms.
- Live streaming partial captions are NOT mapped (avoids transient flicker);
  only the committed final text is mapped.

## Architecture

New pure function in `crates/opendictate-core/src/text.rs`, reusing the
tokenizer pattern already present in `correct_dictionary_terms`:

```rust
pub fn map_spoken_punctuation(text: &str) -> String
```

### Pipeline integration

Offline path (`process_utterance`, `src-tauri/src/dictation.rs` ~469-470):

```
raw → map_spoken_punctuation → correct_dictionary_terms → clean_text
```

Streaming path (endpoint handler, `src-tauri/src/dictation.rs` ~361-373):
apply `map_spoken_punctuation` to the final endpoint text immediately before
`commit_text`. Partials are left raw.

### Settings plumbing

- `Settings` gains `spoken_punctuation: bool` with `#[serde(default)]` (false)
  in `src-tauri/src/state.rs`; `SettingsPatch` gains `Option<bool>`.
- Existing `save_settings` roundtrip + camelCase migration handles persistence.
- New helper `spoken_punctuation_enabled(state)` in `dictation.rs`, mirroring
  `is_continuous_enabled`.

### UI

New row in `src/components/tabs/GeneralTab.tsx` using the existing Switch
pattern (same as "Continuous dictation"):

- Label: "Spoken punctuation"
- Description: "Say 'period', 'comma', 'question mark', or 'exclamation
  point' to insert punctuation."

## Testing

- Unit tests in `opendictate_core::text`:
  - each symbol maps correctly
  - multi-word phrases ("question mark", "exclamation point")
  - decimals preserved ("three point five")
  - no substring hits ("periodontist")
  - mixed sentence capitalization after a mapped `.?!`
- Existing test suite (28 tests) stays green.

## Known limitations

- A rare sentence-starting "Period" (e.g., "Period, this matters") maps to
  ".". Whisper rarely produces this; accepted tradeoff for explicit
  punctuation mode.
- If a user adds "period" to the dictionary, punctuation mapping wins
  (mapping runs before dictionary casing correction).

## Files touched

- `crates/opendictate-core/src/text.rs` — new function + tests
- `src-tauri/src/state.rs` — Settings field
- `src-tauri/src/dictation.rs` — mapping in both paths + helper
- `src/components/tabs/GeneralTab.tsx` — toggle