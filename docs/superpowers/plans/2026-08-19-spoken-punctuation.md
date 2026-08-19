# Spoken Punctuation Control Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a settings toggle, off by default, that converts the spoken words "period", "comma", "question mark", and "exclamation point" into punctuation symbols in both offline and streaming transcription.

**Architecture:** A pure token-based function `map_spoken_punctuation(text: &str) -> String` in `opendictate_core::text` runs early in the text pipeline (before `correct_dictionary_terms` and `clean_text`). A boolean `spoken_punctuation` setting gates it; `dictation.rs` reads the setting and applies the mapping to the offline path and the two streaming commit points. The frontend exposes a Switch in General settings.

**Tech Stack:** Rust (opendictate-core, tauri), TypeScript/React (Vite), zustand store, Tailwind + shadcn/ui Switch.

**Spec:** `docs/superpowers/specs/2026-08-19-spoken-punctuation-design.md`

## Global Constraints

- Commit locally only — do NOT push (pending push approval).
- Rust edition 2021 — no `let chains` (use nested `if let` + `if`).
- Mapping runs FIRST in the pipeline: `raw → map_spoken_punctuation → correct_dictionary_terms → clean_text`.
- "point" is NOT mapped (preserves decimals like "three point five").
- Matching is case-insensitive and standalone-token only — never inside another word.
- Lexicon: `period → .`, `comma → ,`, `question mark → ?`, `exclamation point → !`, `exclamation mark → !`.
- All existing tests (24 core + 4 app) stay green; clippy clean (`cargo clippy --lib --all-features -- -D warnings`).
- Verify with `npm run build` (tsc + vite) after frontend changes.

---
## File Structure

- **Modify** `crates/opendictate-core/src/text.rs` — add `map_spoken_punctuation`; hoist the private `Token`/`tokenize` to module scope so both functions share them; add tests.
- **Modify** `src-tauri/src/state.rs` — add `spoken_punctuation` to `Settings` (serde default false) and `SettingsPatch`.
- **Modify** `src-tauri/src/commands.rs` — apply the patch in `set_settings`.
- **Modify** `src-tauri/src/db.rs` — extend the settings roundtrip/migration test.
- **Modify** `src-tauri/src/dictation.rs` — add `spoken_punctuation_enabled(state)` helper; apply mapping in offline `process_utterance`, streaming endpoint, and `stop_streaming`.
- **Modify** `src/lib/api.ts` — add `spoken_punctuation` to `Settings` and `SettingsPatch`.
- **Modify** `src/components/tabs/GeneralTab.tsx` — add the toggle row.

---

### Task 1: Core `map_spoken_punctuation` function

**Files:**
- Modify: `crates/opendictate-core/src/text.rs`

**Interfaces:**
- Produces: `pub fn map_spoken_punctuation(text: &str) -> String` in `opendictate_core::text`. Later tasks call it as `opendictate_core::text::map_spoken_punctuation(&raw)`.

- [ ] **Step 1: Hoist `Token` and `tokenize` to module scope**

Replace the inner definitions in `correct_dictionary_terms` with module-level ones so `map_spoken_punctuation` can reuse them. At the top of `crates/opendictate-core/src/text.rs`, above `correct_dictionary_terms`, add:

```rust
struct Token {
    start: usize,
    end: usize,
    lower: String,
}

fn tokenize(value: &str) -> Vec<Token> {
    let mut result = Vec::new();
    let mut start = None;
    for (index, character) in value.char_indices() {
        let part_of_word = character.is_alphanumeric() || character == '\'';
        match (start, part_of_word) {
            (None, true) => start = Some(index),
            (Some(begin), false) => {
                let end = index;
                result.push(Token {
                    start: begin,
                    end,
                    lower: value[begin..end].to_lowercase(),
                });
                start = None;
            }
            _ => {}
        }
    }
    if let Some(begin) = start {
        result.push(Token {
            start: begin,
            end: value.len(),
            lower: value[begin..].to_lowercase(),
        });
    }
    result
}
```

Then delete the inner `struct Token` and `fn tokenize` from inside `correct_dictionary_terms` (lines 10-44 in the current file). The function body after that is unchanged and uses the module-level `Token`/`tokenize`.

- [ ] **Step 2: Write the failing tests**

Append to the `mod tests` block in `crates/opendictate-core/src/text.rs`:

```rust
use super::map_spoken_punctuation;

#[test]
fn maps_all_core_punctuation_words() {
    assert_eq!(
        map_spoken_punctuation("period comma question mark exclamation point exclamation mark"),
        ". , ? ! !"
    );
}

#[test]
fn maps_punctuation_mid_sentence() {
    assert_eq!(
        map_spoken_punctuation("hello period this is important comma right question mark"),
        "hello. this is important, right?"
    );
}

#[test]
fn preserves_point_in_decimals() {
    assert_eq!(map_spoken_punctuation("three point five"), "three point five");
}

#[test]
fn does_not_match_inside_words() {
    assert_eq!(map_spoken_punctuation("periodontist"), "periodontist");
}

#[test]
fn matching_is_case_insensitive() {
    assert_eq!(map_spoken_punctuation("Period Comma"), ". ,");
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p opendictate-core --lib`
Expected: FAIL — `error[E0425]: cannot find function 'map_spoken_punctuation'`.

- [ ] **Step 4: Implement `map_spoken_punctuation`**

Add this function to `crates/opendictate-core/src/text.rs`, after `correct_dictionary_terms`:

```rust
/// Maps spoken punctuation words to their symbols. Case-insensitive,
/// standalone-token only — "point" is preserved so decimals survive.
pub fn map_spoken_punctuation(text: &str) -> String {
    const PHRASES: &[(&[&str], char)] = &[
        (&["period"], '.'),
        (&["comma"], ','),
        (&["question", "mark"], '?'),
        (&["exclamation", "point"], '!'),
        (&["exclamation", "mark"], '!'),
    ];

    let tokens = tokenize(text);
    let mut replacements: Vec<(usize, usize, char)> = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        let matched = PHRASES
            .iter()
            .filter_map(|(phrase, symbol)| {
                let end = index + phrase.len();
                (end <= tokens.len()
                    && tokens[index..end]
                        .iter()
                        .map(|token| token.lower.as_str())
                        .eq(phrase.iter().copied()))
                    .then_some((phrase.len(), *symbol))
            })
            .max_by_key(|(length, _)| *length);

        if let Some((length, symbol)) = matched {
            replacements.push((tokens[index].start, tokens[index + length - 1].end, symbol));
            index += length;
        } else {
            index += 1;
        }
    }

    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    for (start, end, symbol) in replacements {
        out.push_str(&text[cursor..start]);
        out.push(symbol);
        cursor = end;
    }
    out.push_str(&text[cursor..]);
    out
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p opendictate-core --lib`
Expected: PASS — 29 passed (24 existing + 5 new), 1 ignored. Existing `correct_dictionary_terms` tests still pass after the tokenizer hoist.

- [ ] **Step 6: Commit**

```bash
git add crates/opendictate-core/src/text.rs
git commit -m "feat(core): map spoken punctuation words to symbols"
```

---

### Task 2: Settings plumbing (backend)

**Files:**
- Modify: `src-tauri/src/state.rs`
- Modify: `src-tauri/src/commands.rs:186-191`
- Modify: `src-tauri/src/db.rs:184-207`

**Interfaces:**
- Consumes: nothing from Task 1.
- Produces: `Settings.spoken_punctuation: bool` and `SettingsPatch.spoken_punctuation: Option<bool>`. Later tasks read it via a settings lock.

- [ ] **Step 1: Add the field to `Settings`**

In `src-tauri/src/state.rs`:

1. In `SettingsPatch` (line ~12-22), after `autostart: Option<bool>,` add:
   ```rust
   pub spoken_punctuation: Option<bool>,
   ```
2. In `Settings` (line ~25-43), after `autostart: bool,` add:
   ```rust
   #[serde(default)]
   pub spoken_punctuation: bool,
   ```
3. In `impl Default for Settings` (line ~61-77), after `autostart: false,` add:
   ```rust
   spoken_punctuation: false,
   ```

- [ ] **Step 2: Apply the patch in `set_settings`**

In `src-tauri/src/commands.rs`, after the `autostart` block (line 189-191):

```rust
    if let Some(autostart) = settings.autostart {
        current.autostart = autostart;
    }
```
add:
```rust
    if let Some(spoken_punctuation) = settings.spoken_punctuation {
        current.spoken_punctuation = spoken_punctuation;
    }
```

- [ ] **Step 3: Extend the migration/roundtrip test**

In `src-tauri/src/db.rs`, in `settings_roundtrip_and_camelcase_migration` (line ~196), after `assert_eq!(s.stt_model, "parakeet-tdt-ctc-110m-int8");` add:

```rust
    assert!(!s.spoken_punctuation);
```

And after the `assert!(!raw.contains("sttModel"));` (line ~206) add:

```rust
    assert!(raw.contains("\"spoken_punctuation\""));
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p opendictate --lib`
Expected: PASS — 4 passed (the extended migration test passes).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/state.rs src-tauri/src/commands.rs src-tauri/src/db.rs
git commit -m "feat: add spoken_punctuation setting"
```

---

### Task 3: Apply mapping in the offline path

**Files:**
- Modify: `src-tauri/src/dictation.rs:436-485`

**Interfaces:**
- Consumes: `opendictate_core::text::map_spoken_punctuation` (Task 1), `Settings.spoken_punctuation` (Task 2).
- Produces: `fn spoken_punctuation_enabled(state: &AppState) -> bool` — later task (Task 4) reuses it.

- [ ] **Step 1: Add the helper**

In `src-tauri/src/dictation.rs`, next to `is_continuous_enabled` (line ~121):

```rust
fn spoken_punctuation_enabled(state: &AppState) -> bool {
    state
        .settings
        .lock()
        .map(|s| s.spoken_punctuation)
        .unwrap_or(false)
}
```

- [ ] **Step 2: Apply mapping in `process_utterance`**

In `process_utterance` (line ~469), replace:

```rust
    let corrected = opendictate_core::text::correct_dictionary_terms(&raw, &dictionary);
    let text = inject::clean_text(&corrected);
```

with:

```rust
    let mapped = if spoken_punctuation_enabled(state) {
        opendictate_core::text::map_spoken_punctuation(&raw)
    } else {
        raw
    };
    let corrected = opendictate_core::text::correct_dictionary_terms(&mapped, &dictionary);
    let text = inject::clean_text(&corrected);
```

- [ ] **Step 3: Compile-check and run tests**

Run: `cargo check -p opendictate && cargo test -p opendictate --lib`
Expected: PASS — compiles, 4 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/dictation.rs
git commit -m "feat: spoken punctuation in offline transcription"
```

---

### Task 4: Apply mapping in the streaming paths

**Files:**
- Modify: `src-tauri/src/dictation.rs:361-373` (endpoint handler)
- Modify: `src-tauri/src/dictation.rs:408-413` (`stop_streaming`)

**Interfaces:**
- Consumes: `spoken_punctuation_enabled` (Task 3), `map_spoken_punctuation` (Task 1).

- [ ] **Step 1: Map the streaming endpoint text**

In the endpoint handler (line ~361), replace:

```rust
            if pipe.recognizer.is_endpoint(&pipe.session) {
                let text = pipe.recognizer.result(&pipe.session);
                let duration_ms = pipe.session.started_at.elapsed().as_millis() as u64;
                if !text.is_empty() {
```

with:

```rust
            if pipe.recognizer.is_endpoint(&pipe.session) {
                let mut text = pipe.recognizer.result(&pipe.session);
                if spoken_punctuation_enabled(&state_from_app(&app)) {
                    text = opendictate_core::text::map_spoken_punctuation(&text);
                }
                let duration_ms = pipe.session.started_at.elapsed().as_millis() as u64;
                if !text.is_empty() {
```

- [ ] **Step 2: Map the `stop_streaming` final text**

In `stop_streaming` (line ~408-413), replace:

```rust
    pipe.recognizer.accept(&pipe.session, &tail);
    let text = pipe.recognizer.result(&pipe.session);
    let duration_ms = pipe.session.started_at.elapsed().as_millis() as u64;
```

with:

```rust
    pipe.recognizer.accept(&pipe.session, &tail);
    let mut text = pipe.recognizer.result(&pipe.session);
    if spoken_punctuation_enabled(state) {
        text = opendictate_core::text::map_spoken_punctuation(&text);
    }
    let duration_ms = pipe.session.started_at.elapsed().as_millis() as u64;
```

- [ ] **Step 3: Compile-check and run tests**

Run: `cargo check -p opendictate && cargo test -p opendictate --lib && cargo clippy --lib --all-features -- -D warnings`
Expected: PASS — compiles, 4 tests pass, clippy clean.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/dictation.rs
git commit -m "feat: spoken punctuation in streaming transcription"
```

---

### Task 5: Frontend types and General settings toggle

**Files:**
- Modify: `src/lib/api.ts:21-48`
- Modify: `src/components/tabs/GeneralTab.tsx`

**Interfaces:**
- Consumes: `SettingsPatch.spoken_punctuation` (Task 2).
- Produces: UI toggle that persists via the existing `api.setSettings`.

- [ ] **Step 1: Add the field to frontend types**

In `src/lib/api.ts`:

1. In `interface Settings` (line ~21-33), after `autostart: boolean;` add:
   ```ts
   spoken_punctuation: boolean;
   ```
2. In the `SettingsPatch` `Pick` union (line ~35-48), after `| "autostart"` add:
   ```ts
   | "spoken_punctuation"
   ```

- [ ] **Step 2: Add the toggle row**

In `src/components/tabs/GeneralTab.tsx`, after the "Start with system" row (line ~266), add a handler and a row. First, next to `handleAutostartChange` (line ~176-181) add:

```ts
  const handleSpokenPunctuationChange = async (enabled: boolean) => {
    try {
      await api.setSettings({ spoken_punctuation: enabled });
      useStore.getState().refreshAll();
    } catch {}
  };
```

Then after the autostart row (the `</div>` closing the "Start with system" block at line ~266), add:

```tsx
      <div className="flex items-center justify-between gap-2">
        <div className="flex flex-col gap-1">
          <Label htmlFor="spoken-punctuation">Spoken punctuation</Label>
          <p className="text-xs text-muted-foreground">
            Say “period”, “comma”, “question mark”, or “exclamation point” to insert punctuation.
          </p>
        </div>
        <Switch
          id="spoken-punctuation"
          checked={settings?.spoken_punctuation ?? false}
          onCheckedChange={handleSpokenPunctuationChange}
        />
      </div>
```

- [ ] **Step 3: Build the frontend**

Run: `npm run build`
Expected: PASS — `tsc && vite build` completes, no type errors.

- [ ] **Step 4: Commit**

```bash
git add src/lib/api.ts src/components/tabs/GeneralTab.tsx
git commit -m "feat: spoken punctuation toggle in settings"
```

---

### Task 6: Full verification and release build

**Files:**
- None (verification only).

- [ ] **Step 1: Run the full test suite**

Run: `cargo test -p opendictate-core --lib && cargo test -p opendictate --lib`
Expected: PASS — 29 core (1 ignored) + 4 app tests.

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --lib --all-features -- -D warnings`
Expected: PASS — no warnings.

- [ ] **Step 3: Release build**

Run: `npm run build && touch src-tauri/src/lib.rs && cargo build --release`
Expected: PASS — release binary built (3-7 min).

- [ ] **Step 4: Restart the app and smoke-test**

Run:
```bash
pkill -x opendictate
rm -f /home/waleed/.local/share/com.opendictate.app/toggle.sock
sleep 1
nohup /home/waleed/Desktop/OpenDictate/target/release/opendictate > /tmp/opencode/app-sp.log 2>&1 &
```

Verify manually: open Settings → General, enable "Spoken punctuation", start a recording (click RECORD), speak "hello period this is a test comma okay question mark", and confirm the inserted text reads "Hello. This is a test, okay?" with correct capitalization.