# OpenDictate — Home Tab Design

Date: 2026-08-20
Status: Approved
Scope: frontend restructure only — rename General → Settings, add Home tab.

## 1. Information architecture

New tab order and naming in `src/App.tsx`:

```
Home (default) · Activity · Dictionary · Snippets · History · Settings
```

- `General` is renamed to **Settings** — identical content (today's `GeneralTab`),
  renamed label, keeps the `Settings` icon.
- New **Home** tab is added first in `TABS` and becomes the default tab on open.
- Header behavior is unchanged: the small global Record button, hotkey badge, and
  recording indicator stay in the header so they are available from any tab.

## 2. Home layout (stacked panels, top → bottom)

1. **Ready strip** — two readiness cards, each with inline remediation:
   - *Microphone*: current mic name or "Not set". Not ready → inline mic `<Select>`
     (reuses `listMics`/`setMic` + `store.mics`).
   - *Model*: selected model + installed/not-installed. Not installed → inline
     download button reusing `ensureModel` + `modelProgress`.
2. **Record button** — prominent variant of the existing `RecordingButton`
   (reuses `startRecording("dictate")`/`stopRecording` + `store.recording`).
3. **Last result panel** — lifts the header's `LastResult` bar into Home:
   text + Undo (`undoLastInsert`) + duration.
4. **Live captions panel** — lifts the header's `LiveCaptions` bar into Home,
   plus a warning banner whenever the selected model is not a streaming model:
   "Live captions require a streaming model — switch to the Parakeet streaming
   model in Settings."

## 3. Data flow

- Zero new backend work — Home reads entirely from the existing store:
  `mic`, `mics`, `catalog`, `models`, `modelProgress`, `recording`, `lastResult`,
  `partial`, `settings`. All already populated by `refreshAll` + event listeners.
- `LastResult` and `LiveCaptions` bars are moved out of the shared main column
  into the Home tab; other tabs get a cleaner view.
- No new IPC commands, no settings changes, no schema changes.

## 4. Components

- New `src/components/tabs/HomeTab.tsx` with small sections: ReadyStrip,
  RecordButton, LastResultPanel, LiveCaptionsPanel.
- `src/App.tsx`: update `TABS`, default tab, tab render block; remove the global
  LastResult + LiveCaptions strips from the main column; keep the header's
  `RecordingButton`.
- `GeneralTab` → `SettingsTab` (file + import rename).

## 5. Edge cases

- Mic not yet chosen → card shows "Not set" + select; default system mic still works.
- Model not installed → inline download with progress; disabled while downloading.
- Recording in progress → Record button shows STOP; switching tabs does not interrupt.
- Streaming warning hidden when model is streaming; shown when no model selected.
- Empty last result / no partial → empty-state text, not blank boxes.

## 6. Verification

- `npm run build` (tsc + vite).
- Manual: open app → lands on Home; mic/model readiness reflect reality;
  record/stop works; undo works; non-streaming model → warning shown;
  streaming model → captions stream; Settings tab has the former General content.