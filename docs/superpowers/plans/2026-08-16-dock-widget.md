# Dock Record Widget Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the transient overlay pill with a persistent, clickable record-button dock (waveform icon) at the top-right screen edge; close-to-tray already works.

**Architecture:** The `overlay` webview window becomes a persistent `dock` window (always-on-top, non-focusable, 140×40, top-right of primary monitor). Rust emits the same `overlay-state` + `audio-level` events; the frontend `DockButton` renders all states (idle/listening/transcribing/inserted/error) and toggles recording on click. No show/hide machinery — the window never hides.

**Tech Stack:** Tauri v2 (Rust), React + Zustand, Tailwind v4 (`@utility` animations already in src/index.css: `animate-od-blink`, `animate-od-bounce-y`, `animate-od-pop`).

## Global Constraints

- Event names stay `overlay-state` and `audio-level` (frontend already listens; API contract unchanged).
- Window must remain `"focusable": false` so paste lands in the user's app, never the dock.
- No auto-push: commit per task, ask user before pushing (batched commits).
- Do not add comments to code (repo convention).

---

### Task 1: Rust backend — persistent dock window

**Files:**
- Modify: `src-tauri/tauri.conf.json:23-37` (overlay window → dock window)
- Create: `src-tauri/src/dock.rs`
- Delete: `src-tauri/src/overlay.rs`
- Modify: `src-tauri/src/lib.rs` (mod decl, setup init, window event, command list)
- Modify: `src-tauri/src/dictation.rs` (overlay → dock)
- Modify: `src-tauri/src/commands.rs` (remove `show_overlay`)

**Interfaces:**
- Produces: `dock::window(app) -> Option<WebviewWindow>`, `dock::init(app)`, `dock::set_state(app, status: &str, message: Option<&str>)` — same `overlay-state` event payload `{state, message}` as before.

- [ ] **Step 1: Update `src-tauri/tauri.conf.json`** — replace the `overlay` window block with:

```json
      {
        "label": "dock",
        "title": "OpenDictate",
        "url": "index.html?window=dock",
        "width": 140,
        "height": 40,
        "transparent": true,
        "decorations": false,
        "alwaysOnTop": true,
        "skipTaskbar": true,
        "focusable": false,
        "resizable": false,
        "shadow": false,
        "visible": true
      }
```

- [ ] **Step 2: Create `src-tauri/src/dock.rs`** with the full content below, then delete `src-tauri/src/overlay.rs`:

```rust
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewWindow};

pub const DOCK_WIDTH: f64 = 140.0;
const MARGIN: f64 = 16.0;

pub fn window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window("dock")
}

fn top_right(app: &AppHandle) -> PhysicalPosition<i32> {
    let default = PhysicalPosition { x: 100, y: 16 };
    let Some(win) = window(app) else {
        return default;
    };
    let Some(monitor) = win.current_monitor().ok().flatten() else {
        return default;
    };
    let size = monitor.size();
    let x = (size.width as f64 - DOCK_WIDTH - MARGIN).max(0.0) as i32;
    PhysicalPosition { x, y: MARGIN as i32 }
}

pub fn init(app: &AppHandle) {
    if let Some(win) = window(app) {
        let _ = win.set_position(top_right(app));
        let _ = win.set_always_on_top(true);
        let _ = win.show();
    }
}

pub fn set_state(app: &AppHandle, status: &str, message: Option<&str>) {
    let _ = app.emit(
        "overlay-state",
        serde_json::json!({
            "state": status,
            "message": message,
        }),
    );
}
```

- [ ] **Step 3: Update `src-tauri/src/lib.rs`** — three edits:

1. `mod overlay;` → `mod dock;`
2. In `.setup(...)`: replace `overlay::hide(handle);` with `dock::init(handle);`
3. In the invoke handler list: remove `commands::show_overlay,`
4. Replace the `.on_window_event(...)` block with:

```rust
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                match window.label() {
                    "main" => {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                    "dock" => api.prevent_close(),
                    _ => {}
                }
            }
        })
```

- [ ] **Step 4: Update `src-tauri/src/dictation.rs`** — `use crate::overlay;` → `use crate::dock;` and replace all `overlay::set_state(` occurrences with `dock::set_state(` (6 call sites).

- [ ] **Step 5: Update `src-tauri/src/commands.rs`** — delete the `show_overlay` command (lines 202-205).

- [ ] **Step 6: Verify** — run `cargo build` then `cargo test` then `cargo clippy -- -D warnings` in `src-tauri/`. Expected: builds clean, all tests pass, clippy 0 warnings.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/
git commit -m "feat: persistent dock window replaces transient overlay pill"
```

---

### Task 2: Frontend — DockButton widget

**Files:**
- Create: `src/components/DockButton.tsx`
- Delete: `src/components/OverlayPill.tsx`
- Modify: `src/App.tsx` (OverlayApp → DockApp, renders DockButton)
- Modify: `src/main.tsx` (window routing: label/query `dock`)
- Modify: `src/App.tsx:46` (fold in earlier `result?.text` hardening — already applied)

**Interfaces:**
- Consumes: store fields `level`, `overlayState`, `recording`; api `startRecording("dictate")`, `stopRecording()`, `onOverlayState`, `onAudioLevel` (via existing `useOpenDictateEvents`).

- [ ] **Step 1: Create `src/components/DockButton.tsx`**:

```tsx
import { useEffect, useRef, useState, type ReactNode } from "react";
import { useStore } from "@/lib/store";
import * as api from "@/lib/api";

const BAR_COUNT = 18;
const BAR_WIDTH = 3;
const BAR_GAP = 2;
const BUFFER_SIZE = 32;

function Waveform({ active }: { active: boolean }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const bufferRef = useRef<number[]>([]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    let raf = 0;
    const draw = () => {
      const level = active ? useStore.getState().level : 0.22;
      const buffer = bufferRef.current;
      buffer.push(level);
      if (buffer.length > BUFFER_SIZE) buffer.shift();

      const w = canvas.width;
      const h = canvas.height;
      ctx.clearRect(0, 0, w, h);
      ctx.fillStyle = "#000000";

      const half = Math.floor(BAR_COUNT / 2);
      for (let i = 0; i < half; i++) {
        const sample =
          buffer[Math.floor((i / half) * Math.max(buffer.length - 1, 0))] ?? 0;
        const barHeight = Math.max(2, Math.min(h, sample * h));
        const xLeft = (half - 1 - i) * (BAR_WIDTH + BAR_GAP);
        const xRight = w - (half - i) * (BAR_WIDTH + BAR_GAP);
        const yMid = (h - barHeight) / 2;
        ctx.fillRect(xLeft, yMid, BAR_WIDTH, barHeight);
        ctx.fillRect(xRight, yMid, BAR_WIDTH, barHeight);
      }
      raf = requestAnimationFrame(draw);
    };
    raf = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(raf);
  }, [active]);

  return (
    <canvas
      ref={canvasRef}
      width={BAR_COUNT * BAR_WIDTH + (BAR_COUNT - 1) * BAR_GAP}
      height={26}
    />
  );
}

export function DockButton() {
  const overlayState = useStore((s) => s.overlayState);
  const recording = useStore((s) => s.recording);
  const [error, setError] = useState<string | null>(null);
  const [flash, setFlash] = useState<"inserted" | "error" | null>(null);
  const flashTimer = useRef<number | null>(null);

  const state = overlayState?.state ?? "hidden";
  const active = state === "listening" || recording;
  const canStop = state === "listening";

  useEffect(() => {
    if (state === "inserted") {
      setFlash("inserted");
      flashTimer.current = window.setTimeout(() => setFlash(null), 1400);
    } else if (state === "error") {
      setFlash("error");
      flashTimer.current = window.setTimeout(() => setFlash(null), 2400);
    }
    return () => {
      if (flashTimer.current) window.clearTimeout(flashTimer.current);
    };
  }, [state]);

  const toggle = async () => {
    setError(null);
    if (canStop) {
      try {
        const result = await api.stopRecording();
        if (result?.text) {
          useStore.setState({ lastResult: result });
        }
      } catch (e) {
        setError(String(e));
      }
      useStore.getState().setRecording(false);
    } else if (!active) {
      try {
        await api.startRecording("dictate");
        useStore.getState().setRecording(true);
      } catch (e) {
        setError(String(e));
      }
    }
  };

  let content: ReactNode;
  if (flash === "inserted") {
    content = (
      <div className="flex h-10 w-[140px] animate-od-pop items-center justify-center gap-2 border-2 border-black bg-black text-white">
        <span className="text-[11px] font-bold tracking-[0.2em] uppercase">
          Inserted
        </span>
        <span className="text-sm font-bold">✓</span>
      </div>
    );
  } else if (flash === "error" || error) {
    content = (
      <div className="flex h-10 w-[140px] animate-od-pop items-center justify-center gap-2 border-2 border-black bg-black px-2 text-white">
        <span className="truncate text-[10px] font-bold tracking-wider uppercase">
          ✕ {error ?? "Error"}
        </span>
      </div>
    );
  } else if (state === "transcribing") {
    content = (
      <div className="flex h-10 w-[140px] items-center justify-center gap-2 border-2 border-black bg-white">
        <span className="flex gap-1">
          {[0, 1, 2].map((i) => (
            <span
              key={i}
              className="size-1.5 animate-od-bounce-y bg-black"
              style={{ animationDelay: `${i * 0.15}s` }}
            />
          ))}
        </span>
        <span className="text-[11px] font-bold tracking-[0.2em] uppercase">
          Transcribing
        </span>
      </div>
    );
  } else {
    content = (
      <div
        className={`flex h-10 w-[140px] cursor-pointer items-center justify-center gap-2.5 border-2 border-black bg-white px-3 transition-transform hover:scale-[1.03] ${active ? "" : "opacity-85"}`}
      >
        {active && (
          <span className="size-2 animate-od-blink border-2 border-black bg-black" />
        )}
        <Waveform active={active} />
      </div>
    );
  }

  return (
    <button
      type="button"
      onClick={toggle}
      className="fixed inset-0 cursor-pointer"
      aria-label={canStop ? "Stop recording" : "Start recording"}
      title={canStop ? "Stop recording" : "Start recording"}
    >
      {content}
    </button>
  );
}
```

- [ ] **Step 2: Update `src/App.tsx`** — replace the `OverlayApp` export (lines 181-189) with:

```tsx
export function DockApp() {
  useOpenDictateEvents();

  return (
    <div className="fixed inset-0">
      <DockButton />
    </div>
  );
}
```

Add `import { DockButton } from "@/components/DockButton";` and remove the `OverlayPill` import.

- [ ] **Step 3: Update `src/main.tsx`** — rename routing:

```tsx
import { MainApp, DockApp } from "./App";
...
async function main() {
  let isDock = false;
  try {
    isDock = getCurrentWindow().label === "dock";
  } catch {
    isDock = new URLSearchParams(window.location.search).get("window") === "dock";
  }

  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      {isDock ? <DockApp /> : <MainApp />}
    </React.StrictMode>,
  );
}
```

- [ ] **Step 4: Delete `src/components/OverlayPill.tsx`**.

- [ ] **Step 5: Verify** — run `npm run build` (tsc + vite). Expected: no type errors, build succeeds.

- [ ] **Step 6: Commit**

```bash
git add src/
git commit -m "feat: dock record button widget with live waveform states"
```

---

### Task 3: Verify end-to-end

**Files:**
- Modify: `/tmp/opencode/stub-invoke.js` (record flow already stubbed from earlier session)

- [ ] **Step 1: Restart the real app** — kill PID of running app, relaunch `nohup npm run tauri dev > /tmp/opencode/tauri-dev9.log 2>&1 &`, wait for "hotkey registered" and "recording started" readiness in log.

- [ ] **Step 2: Stub verification** — reload the Playwright stub page (`http://localhost:1420` with stub injected via `addInitScript`). Click the dock button → assert visual switches to recording (blinking dot), click again → assert no TypeError and idle returns. Check console for errors.

- [ ] **Step 3: Rust tests** — `cargo test` (src-tauri) and `cargo test` (crates/opendictate-core): all green.

- [ ] **Step 4: Report to user** — summarize changes + ask user to test in the real app: close window (app stays in tray), click dock / hotkey ctrl+k to record, speak, stop → text appears in focused app. Ask whether they want a commit/push batch (do not push without approval).
