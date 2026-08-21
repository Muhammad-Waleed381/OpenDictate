# UI/UX Polish Pass — "Softened Brutalism" Design

Date: 2026-08-21
Status: Approved (design dialogue 2026-08-21)

## Goal

Evolve OpenDictate's neo-brutalist UI into a polished, consistent whole: keep the
identity (2px borders, offset shadows, Space Grotesk, uppercase micro-labels),
soften interactive elements, add real dark mode, and close every feedback gap
(silent failures, no transient confirmation, native confirms, missing loading
states).

Non-goals: changing the dock overlay's visual language (it is an OS-level
overlay and is intentionally distinct), routing/URL navigation, new features.

## 1. Visual Language

### Radius policy
- Interactive controls (`Button`, `Input`, `Textarea`, `Select` trigger,
  `Slider`) get `rounded-md`.
- Cards, tables, banners keep sharp corners.
- Badges stay square. Dock unchanged.

### Shadow policy
- Standard card shadow: `3px 3px 0 0 var(--shadow-color)` where
  `--shadow-color: #1a1a1a` in light, transparent black in dark.
- Primary CTA keeps full-strength shadow; hoverable cards get reduced-offset
  shadow + translate on hover (existing pattern, standardized).
- Dark mode: offset shadows become `0 0 0 1px` border + subtle
  `4px 4px 12px rgba(0,0,0,.45)` glow.

### Accent color
- New accent token `--accent: #2545d3` (ink blue) with hover/dim variants.
- Used for: focus-visible rings, active nav indicator, links, selected states,
  slider fill. Recording red unchanged.

## 2. Dark Mode

- Real token set under `.dark` (surfaces `#111111` / `#1a1a1a`, text
  `#f2f2f2`, borders `#3a3a3a`). All existing components consume tokens only —
  audit removes hardcoded `bg-black`, `text-white`, `#000` shadow utilities.
- Toggle: Settings switch "Dark mode" with three-state default
  `system | light | dark`; persisted via zustand store (same channel as other
  settings); applied by setting/removing `.dark` on `<html>` at startup and on
  change; system option tracks `prefers-color-scheme`.
- Sidebar stops being hardcoded black; uses surface tokens in both modes.

## 3. New Primitives (src/components/ui)

| Primitive   | Notes |
|-------------|-------|
| toast       | zustand store + `<Toaster/>` mounted in App; variants success/error/info; auto-dismiss 4s (errors 6s); stack bottom-right; brutal styling |
| confirm-dialog | `useConfirm()` store API returning a promise; destructive variant (red button); replaces all 4 `window.confirm` sites |
| textarea    | styled like Input; replaces 4 raw `<textarea>` copies |
| slider      | themed range over @base-ui or styled native input; accent fill |
| skeleton    | pulse block used for loading states |
| card        | adopt existing `ui/card.tsx` as standard container everywhere |

## 4. Behavior / UX Fixes

- **useRecording hook** (`src/lib/useRecording.ts`): single implementation of
  toggle/start/stop + error surfacing; consumed by App header button, HomeTab
  record button, (DockButton keeps its own minimal path — separate window).
- **Feedback everywhere**: copy/re-insert/delete/export → success/error toasts;
  errors route through correct channels (fix HistoryTab edit-error using
  exportError state); remove all empty `catch {}` (DictionaryTab ×4, HomeTab
  mic-select, HeatmapTab reset).
- **Loading states**: skeletons for settings hydration, history table, activity
  stats on first load.
- **Empty states**: Activity tab gets explicit "no data yet" guidance panel.
- **Row actions**: Snippets + History rows use identical icon buttons with
  tooltips; Delete uses destructive variant; Cancel button variant unified.
- **Nav**: active tab gets accent left-indicator bar; focus-visible rings
  verified on all interactive elements.

## 5. Architecture

- Tokens live only in `src/index.css` (`:root`, `.dark`).
- Toast + confirm stores in `src/lib/` alongside existing zustand stores.
- No backend/Tauri command changes required (except none known today).

## 6. Testing & Verification

- `pnpm build` (tsc + vite) passes.
- Manual pass per tab in light + dark: Home, Activity, Dictionary, Snippets,
  History, Settings, Onboarding dialog, MicTest.
- Grep gates: no remaining `window.confirm`, no empty `catch {}` in tabs, no
  raw `<textarea>` outside the Textarea primitive, no `bg-black` outside
  intentional logo/dock usage.

## Risks

- Token audit may miss hardcoded colors → mitigated by grep gate above.
- Dark-mode contrast on the heatmap presets → verify each preset in both modes.
