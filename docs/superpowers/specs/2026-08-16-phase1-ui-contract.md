# OpenDictate — Phase 1 UI / IPC Contract

Date: 2026-08-16
Status: Working contract — frontend and backend MUST agree on this exactly.

## 1. Windows (tauri.conf.json)

| Label | Purpose | Key attrs |
|---|---|---|
| `main` | Settings UI, 800×600, dark | title "OpenDictate" |
| `overlay` | Focus-less pill, 360×76, transparent | `url: "index.html?window=overlay"`, alwaysOnTop, skipTaskbar, focusable:false, decorations:false, resizable:false, shadow:false, visible:false (shown by Rust) |

Frontend routing: `main.tsx` reads `location.search` — if `window=overlay` render `<OverlayApp/>`, else `<MainApp/>`.

## 2. Tauri events (Rust → JS, via `@tauri-apps/api/event`)

| Event | Payload | Meaning |
|---|---|---|
| `overlay-state` | `{ state: "listening" \| "transcribing" \| "inserted" \| "error" \| "hidden", message?: string }` | Pill state; Rust shows/hides overlay window |
| `audio-level` | `{ rms: number }` | 0..1 live mic level, ~30 fps while listening (drives waveform + mic test) |
| `model-progress` | `{ file: string, received: number, total: number }` | Model download progress |
| `models-ready` | `{}` | All models present |
| `transcript` | `{ text: string, injected: boolean }` | Final transcript (injected = pasted into other app) |

## 3. Commands (JS → Rust, via `invoke`)

| Command | Args | Returns |
|---|---|---|
| `list_mics` | — | `string[]` — real microphone names only; ALSA plugin/remap devices (lavrate, pipewire, pulse, dsnoop:…, hw:…, …) are filtered out; `"default"` is kept and labeled "System default (built-in)" by the UI |
| `get_mic` | — | current mic device name (or `null`) |
| `get_mic` | — | `string \| null` |
| `set_mic` | `name: string` | `()` |
| `models_status` | — | `{ stt_ready: boolean, vad_ready: boolean }` |
| `models_catalog` | — | `{ id, name, kind: "stt" \| "vad", engine_key: string \| null, size_bytes, disk_bytes, installed, available }[]` — every downloadable/installed model with real on-disk size. The UI shows STT models only; VAD is auto-managed (installed at startup, never user-configurable) |
| `ensure_model` | `id: string` | `()` (emits `model-progress` with `file` = model id; emits `models-ready` on success; errors throw) |
| `remove_model` | `id: string` | `()` — deletes model files from disk |
| `start_recording` | `mode: "dictate" \| "test"` | `()` |
| `stop_recording` | — | `{ text: string, duration_ms: number }` — "test" mode returns raw text only, no inject; "dictate" mode injects into focused app and emits `transcript` |
| `cancel_recording` | — | `()` |
| `is_recording` | — | `boolean` |
| `get_settings` | — | `{ hotkey: string, mic: string \| null, engine: string, language: string, onboarded: boolean, stt_model: string }` |
| `set_settings` | `settings` (partial: hotkey, engine, language, stt_model) | `()` — re-registers hotkey if changed (new key registered first; old stays live on failure) |
| `get_history` | — | `{ id: number, text: string, created_at: string, duration_ms: number, source: string }[]` |
| `delete_history` | `id: number` | `()` |
| `clear_history` | — | `()` |
| `get_dictionary` | — | `{ id: number, word: string, created_at: string }[]` |
| `add_dictionary_word` | `word: string` | `()` |
| `remove_dictionary_word` | `word: string` | `()` |
| `paste_clipboard` | `text: string` | `()` — clipboard save → paste → restore (used by History copy) |

Hotkey flow (Rust-internal): global hotkey toggles dictate-mode recording; on stop, transcribe → inject → `overlay-state: inserted` → hide after ~1.2 s. Errors → `overlay-state: error` with message (never silent).

## 4. Design tokens (spec §4.2 — dark-first, applies to main window AND overlay)

| Token | Value | Use |
|---|---|---|
| bg | `#0F172A` (slate-900) | window background |
| bg-card | `#1E293B` (slate-800) | cards, inputs |
| border | `#334155` (slate-700) | borders |
| text | `#F8FAFC` (slate-50) | primary text |
| muted | `#64748B` (slate-500) | secondary text |
| primary/listening | `#3B82F6` (blue-500) | accents, recording state |
| success/inserted | `#10B981` (emerald-500) | inserted state |
| danger | `#EF4444` (red-500) | errors |
| command (v0.2) | `#A855F7` (purple-500) | reserved |

Main window chrome: header row with app name + recording state dot + hotkey badge; content below. Tabs: **General · Dictionary · History · Privacy** (shadcn Tabs). Footer: version + "local-first, zero telemetry" tagline.

## 5. Main window tabs (spec §4.2)

- **General**: mic `<Select>` (from `list_mics`), engine radio/select (Parakeet only; Whisper shown disabled "coming soon"), model status card (ready badge or Download button with `<Progress>` fed by `model-progress`), hotkey display + apply (input, shorthand like `Ctrl+Alt+Space`; Rust re-registers), language select (auto only, disabled others).
- **Dictionary**: list of words (badges + remove ×), add input + button (empty words rejected; backend trims/lowercases).
- **History**: searchable table (`text`, `date`, `source`, actions copy/delete), clear-all button.
- **Privacy**: static card listing all network endpoints (model downloads only — huggingface/github release; zero outbound calls otherwise) + "no telemetry" statement.

## 6. Onboarding (first-run, only if `onboarded == false`)

Modal dialog, 3 steps, progress indicator:
1. **Mic test** — live level meter (subscribes `audio-level`), Start/Stop test button (mode `"test"`), shows peak + verdict (mic working / too quiet).
2. **Model download** — status per file with progress bars; Download button if not ready; auto-continue when ready (listens `models-ready`).
3. **Hotkey confirm** — shows hotkey badge + explanation; Done button → `set_settings({ onboarded: true })` — NOTE: `onboarded` is READ-ONLY via set_settings; add nothing, instead the Done button calls `complete_onboarding` command. (Contract addition: command `complete_onboarding` → `()`.)

## 7. Overlay pill (overlay window)

Fixed 360×76 transparent pill, top-center. Renders only the current state:
- `listening`: blue border/glow, "Listening…" + canvas waveform (RMS from `audio-level`, 30fps, mirrored bars)
- `transcribing`: subtle "Transcribing…" with thin progress shimmer
- `inserted`: green flash "Inserted"
- `error`: red pill, message text
- `hidden`: nothing (window hidden by Rust)
No buttons, no focus, pointer-events none (CSS), rounded-full.

## 8. Backend pipeline (Rust)

`start(mode)` → cpal capture (16 kHz mono, RAM only) → `audio-level` events (RMS per ~100 ms) → `stop()` → Silero VAD trim → Parakeet STT (~1–2 s) → `dictate` mode: clipboard save → paste (xdotool Ctrl+V on X11 / wtype on Wayland) → restore after 300 ms → history insert → `transcript` + `overlay-state`. `test` mode: skip inject/history, return text. Esc/errors → `overlay-state: error` with actionable message. Settings + history + dictionary in single SQLite file `opendictate.db` (app-data dir). Hotkey default `Ctrl+Alt+Space` (tauri-plugin-global-shortcut).

## 9. Files (frontend layout)

```
src/
  main.tsx            — window routing (?window=overlay)
  App.tsx             — MainApp: header + tabs + onboarding
  index.css           — tailwind v4 + tokens
  lib/api.ts          — typed invoke wrappers + event subscriptions
  lib/store.ts        — zustand store (level, overlayState, models, settings, history, dict, recording)
  components/OverlayPill.tsx
  components/Onboarding.tsx
  components/MicTest.tsx
  components/ModelCard.tsx
  components/tabs/GeneralTab.tsx
  components/tabs/DictionaryTab.tsx
  components/tabs/HistoryTab.tsx
  components/tabs/PrivacyTab.tsx
  components/ui/*      — shadcn components (button, card, tabs, input, select, switch, badge, table, progress, dialog, label, scroll-area, separator)
```

All IPC types in `lib/api.ts` mirror §2/§3 exactly (camelCase payload fields → Rust serde rename_all camelCase).