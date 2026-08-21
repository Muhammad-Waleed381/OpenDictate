# UI/UX Polish Pass Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Evolve the neo-brutalist UI into a polished whole: real dark mode, softened interactive controls, ink-blue accent, and closed feedback gaps (toasts, confirm dialogs, skeletons, unified row actions).

**Architecture:** Token-first — all colors/radii/shadows live in `src/index.css` (`:root` / `.dark`). New primitives (`toast`, `confirm-dialog`, `textarea`, `slider`, `skeleton`) follow the existing shadcn-style pattern in `src/components/ui/`. A shared `useRecording` hook replaces three copies of record logic. Theme preference persists via zustand + localStorage (no backend changes).

**Tech Stack:** React 19, Tailwind v4 (CSS-first config), @base-ui/react, cva, zustand 5, lucide-react, Tauri 2. Build: `npm run build` (= `tsc && vite build`). No test runner configured — verification is typecheck/build + grep gates + manual pass.

## Global Constraints

- Spec: `docs/superpowers/specs/2026-08-21-ui-ux-polish-design.md`
- Dock overlay (`DockApp`, `DockButton.tsx`, `html.dock-page` CSS) is visually untouched.
- Brand accent: `--brand: #2545D3` light / `#8FA6FF` dark; `--brand-foreground: #FFFFFF` light / `#0A1030` dark.
- Destructive becomes red: `#C62828` light / `#EF5350` dark; foreground `#FFFFFF`.
- Radius: controls only get radius (`--radius-sm: 0.25rem`, `--radius-md: 0.375rem`; lg/xl stay 0). Cards/badges stay square.
- Shadow color token: `--od-shadow: #1a1a1a` light; dark uses glow instead of offset shadow.
- Never modify files under `src-tauri/` in this plan.
- Commit after every task with the exact message given.

---

### Task 1: Design tokens — radius, shadows, accent, destructive, real dark set

**Files:**
- Modify: `src/index.css`
- Modify: `src/main.tsx`

**Interfaces:**
- Produces: CSS custom properties `--od-shadow`, `--brand`, `--brand-foreground`; theme utilities `shadow-brutal`, `shadow-brutal-sm`; a working `.dark` token block consumed by every later task.
- Produces: `main.tsx` no longer force-adds `.dark`.

- [ ] **Step 1: Update the `@theme inline` block**

In `src/index.css`, change the radius lines inside `@theme inline` (currently lines 43–49, all `0`) to:

```css
    --radius-sm: 0.25rem;
    --radius-md: 0.375rem;
    --radius-lg: 0;
    --radius-xl: 0;
    --radius-2xl: 0;
    --radius-3xl: 0;
    --radius-4xl: 0;
```

Add these two mappings right after the `--color-background: var(--background);` line:

```css
    --color-brand: var(--brand);
    --color-brand-foreground: var(--brand-foreground);
```

- [ ] **Step 2: Replace the `:root` block**

Replace the entire `:root { ... }` block (lines 54–88) with:

```css
:root {
    --background: #FFFFFF;
    --foreground: #000000;
    --card: #FFFFFF;
    --card-foreground: #000000;
    --popover: #FFFFFF;
    --popover-foreground: #000000;
    --primary: #000000;
    --primary-foreground: #FFFFFF;
    --secondary: #F2F2F2;
    --secondary-foreground: #000000;
    --muted: #F2F2F2;
    --muted-foreground: #525252;
    --accent: #E8E8E8;
    --accent-foreground: #000000;
    --brand: #2545D3;
    --brand-foreground: #FFFFFF;
    --destructive: #C62828;
    --destructive-foreground: #FFFFFF;
    --border: #000000;
    --input: #000000;
    --ring: #2545D3;
    --chart-1: #000000;
    --chart-2: #333333;
    --chart-3: #666666;
    --chart-4: #999999;
    --chart-5: #CCCCCC;
    --radius: 0rem;
    --sidebar: #FFFFFF;
    --sidebar-foreground: #000000;
    --sidebar-primary: #000000;
    --sidebar-primary-foreground: #FFFFFF;
    --sidebar-accent: #E8E8E8;
    --sidebar-accent-foreground: #000000;
    --sidebar-border: #000000;
    --sidebar-ring: #2545D3;
    --od-shadow: #1a1a1a;
}
```

- [ ] **Step 3: Replace the `.dark` block with the real dark set**

```css
.dark {
    --background: #111111;
    --foreground: #F2F2F2;
    --card: #1A1A1A;
    --card-foreground: #F2F2F2;
    --popover: #1A1A1A;
    --popover-foreground: #F2F2F2;
    --primary: #F2F2F2;
    --primary-foreground: #111111;
    --secondary: #262626;
    --secondary-foreground: #F2F2F2;
    --muted: #262626;
    --muted-foreground: #A3A3A3;
    --accent: #262626;
    --accent-foreground: #F2F2F2;
    --brand: #8FA6FF;
    --brand-foreground: #0A1030;
    --destructive: #EF5350;
    --destructive-foreground: #FFFFFF;
    --border: #3A3A3A;
    --input: #3A3A3A;
    --ring: #8FA6FF;
    --chart-1: #F2F2F2;
    --chart-2: #C9C9C9;
    --chart-3: #999999;
    --chart-4: #666666;
    --chart-5: #3D3D3D;
    --radius: 0rem;
    --sidebar: #161616;
    --sidebar-foreground: #F2F2F2;
    --sidebar-primary: #F2F2F2;
    --sidebar-primary-foreground: #111111;
    --sidebar-accent: #262626;
    --sidebar-accent-foreground: #F2F2F2;
    --sidebar-border: #3A3A3A;
    --sidebar-ring: #8FA6FF;
    --od-shadow: rgba(0, 0, 0, 0.55);
}
```

- [ ] **Step 4: Rewrite the brutal component classes to use tokens + add dark glow**

Replace the `.brutal*` classes in `@layer components` (lines 150–168) with:

```css
  .brutal {
    @apply border-2 border-border;
    box-shadow: 4px 4px 0 0 var(--od-shadow);
  }
  .brutal-sm {
    @apply border-2 border-border;
    box-shadow: 2px 2px 0 0 var(--od-shadow);
  }
  .brutal-lg {
    @apply border-2 border-border;
    box-shadow: 8px 8px 0 0 var(--od-shadow);
  }
  .brutal-pressed {
    transform: translate(2px, 2px);
    box-shadow: 0 0 0 0 var(--od-shadow);
  }
  .dark .brutal,
  .dark .brutal-sm,
  .dark .brutal-lg {
    box-shadow: 4px 4px 12px var(--od-shadow);
  }
  .dark .brutal-pressed {
    transform: none;
    box-shadow: none;
  }
  .brutal-inset {
    box-shadow: inset 3px 3px 0 0 var(--accent);
  }
```

- [ ] **Step 5: Add shadow utilities after the existing `@utility` blocks**

Append at the end of `src/index.css`:

```css
@utility shadow-brutal {
  box-shadow: 3px 3px 0 0 var(--od-shadow);
}

@utility shadow-brutal-hover {
  box-shadow: 1px 1px 0 0 var(--od-shadow);
}

@utility shadow-brutal-none {
  box-shadow: 0 0 0 0 var(--od-shadow);
}
```

- [ ] **Step 6: Remove forced dark class from main.tsx**

In `src/main.tsx`, delete line 7 entirely:

```ts
document.documentElement.classList.add("dark");
```

(Re-application of `.dark` per user preference arrives in Task 2's theme store; until then the app renders light.)

- [ ] **Step 7: Verify build**

Run: `npm run build`
Expected: compiles with no TypeScript errors.

- [ ] **Step 8: Commit**

```bash
git add src/index.css src/main.tsx
git commit -m "feat(ui): real design tokens — radius on controls, brand accent, red destructive, true dark palette"
```

---

### Task 2: Theme store + Settings appearance control

**Files:**
- Create: `src/lib/theme.ts`
- Modify: `src/App.tsx`
- Modify: `src/components/tabs/SettingsTab.tsx`
- Modify: `src/components/ui/select.tsx` (only if Select trigger hardcodes styles — inspect first)

**Interfaces:**
- Produces: `useTheme()` hook from `@/lib/theme` returning `{ pref: "system" | "light" | "dark"; resolved: "light" | "dark"; setPref }`. Applies/removes `.dark` on `<html>`.
- Consumes: nothing from other tasks.

- [ ] **Step 1: Create `src/lib/theme.ts`**

```ts
import { create } from "zustand";

type ThemePref = "system" | "light" | "dark";

interface ThemeStore {
  pref: ThemePref;
  setPref: (pref: ThemePref) => void;
}

const STORAGE_KEY = "od:theme";

function loadPref(): ThemePref {
  const value = localStorage.getItem(STORAGE_KEY);
  return value === "light" || value === "dark" || value === "system"
    ? value
    : "system";
}

export const useTheme = create<ThemeStore>()((set) => ({
  pref: loadPref(),
  setPref: (pref) => {
    localStorage.setItem(STORAGE_KEY, pref);
    applyTheme(pref);
    set({ pref });
  },
}));

function systemDark(): boolean {
  return window.matchMedia("(prefers-color-scheme: dark)").matches;
}

export function applyTheme(pref: ThemePref): void {
  const root = document.documentElement;
  root.classList.toggle("dark", pref === "dark" || (pref === "system" && systemDark()));
}

export function initTheme(): void {
  applyTheme(useTheme.getState().pref);
  window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
    if (useTheme.getState().pref === "system") applyTheme("system");
  });
}
```

- [ ] **Step 2: Call `initTheme()` in main.tsx**

Inside `async function main() { ... }` in `src/main.tsx`, before the dock/main branch logic, add:

```ts
import { initTheme } from "@/lib/theme";
```

and as the first statement of `main()`:

```ts
initTheme();
```

Note: keep the dock window always in whatever theme resolves — the dock is unaffected because it never sets `.dark` itself; if the dock window should stay light, guard: `if (!isDock) initTheme();` after `isDock` is computed. Use this guarded form.

- [ ] **Step 3: Add Appearance section to SettingsTab**

In `src/components/tabs/SettingsTab.tsx`, add imports at top:

```tsx
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Label } from "@/components/ui/label";
import { useTheme } from "@/lib/theme";
```

(omit any already present). Inside the component body add:

```tsx
const themePref = useTheme((s) => s.pref);
const setThemePref = useTheme((s) => s.setPref);
```

At the top of the returned JSX (as the first child of the outer flex column), insert:

```tsx
      <div className="flex items-center justify-between gap-2">
        <div className="flex flex-col gap-1">
          <Label htmlFor="theme-select">Appearance</Label>
          <p className="text-xs text-muted-foreground">
            Follow the system setting or pick light / dark.
          </p>
        </div>
        <Select value={themePref} onValueChange={(v) => setThemePref(v as "system" | "light" | "dark")}>
          <SelectTrigger id="theme-select" className="w-32">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="system">System</SelectItem>
            <SelectItem value="light">Light</SelectItem>
            <SelectItem value="dark">Dark</SelectItem>
          </SelectContent>
        </Select>
      </div>
```

- [ ] **Step 4: Un-hardcode sidebar/header/footer in App.tsx**

In `src/App.tsx`:
- Header (line ~110): change `border-b-2 border-black bg-black px-6 py-3 text-white` → `border-b-2 border-border bg-card px-6 py-3 text-foreground`; change inner logo span classes `border-white bg-white ... text-black` → `border-border bg-primary ... text-primary-foreground`; the hotkey Badge `border-white text-white` → `border-border text-foreground`; recording dot `border-white` → `border-foreground`, `bg-white` → `bg-foreground`.
- Sidebar aside (line ~167): change `border-r-2 border-black bg-black` → `border-r-2 border-sidebar-border bg-sidebar`; collapse button `text-white/70 hover:bg-white/10 hover:text-white` and `border-white/10` → `text-muted-foreground hover:bg-accent hover:text-accent-foreground` and `border-sidebar-border`; nav buttons active state `bg-white text-black` → `bg-brand text-brand-foreground`, inactive `text-white/70 hover:bg-white/10 hover:text-white` → `text-sidebar-foreground/70 hover:bg-sidebar-accent hover:text-sidebar-foreground`; footer block `text-white/40 border-white/10` → `text-muted-foreground border-sidebar-border`.
- Footer (line ~218): `border-black bg-black ... text-white` → `border-border bg-card ... text-foreground`, and `text-white/60` → `text-muted-foreground`.

- [ ] **Step 5: Verify build + manual check**

Run: `npm run build`
Expected: clean. Then `npm run tauri dev`, open Settings → switch System/Light/Dark and confirm the app flips; confirm sidebar/header/footer adapt.

- [ ] **Step 6: Commit**

```bash
git add src/lib/theme.ts src/main.tsx src/App.tsx src/components/tabs/SettingsTab.tsx
git commit -m "feat(ui): working dark mode — theme store, settings control, tokenized chrome"
```

---

### Task 3: Toast primitive

**Files:**
- Create: `src/components/ui/toast.tsx`
- Modify: `src/App.tsx` (mount `<Toaster />`)

**Interfaces:**
- Produces: `toast.success(message: string)`, `toast.error(message: string)`, `toast.info(message: string)` — callable from anywhere (zustand store outside React).
- Produces: `<Toaster />` component rendering stacked toasts bottom-right.

- [ ] **Step 1: Create `src/components/ui/toast.tsx`**

```tsx
import { create } from "zustand";
import { cn } from "@/lib/utils";

type ToastKind = "success" | "error" | "info";

interface ToastItem {
  id: number;
  kind: ToastKind;
  message: string;
}

interface ToastStore {
  items: ToastItem[];
  push: (kind: ToastKind, message: string) => void;
  dismiss: (id: number) => void;
}

let nextId = 1;

export const useToastStore = create<ToastStore>()((set) => ({
  items: [],
  push: (kind, message) => {
    const id = nextId++;
    set((state) => ({ items: [...state.items.slice(-4), { id, kind, message }] }));
    const ttl = kind === "error" ? 6000 : 4000;
    setTimeout(() => set((state) => ({ items: state.items.filter((t) => t.id !== id) })), ttl);
  },
  dismiss: (id) =>
    set((state) => ({ items: state.items.filter((t) => t.id !== id) })),
}));

export const toast = {
  success: (message: string) => useToastStore.getState().push("success", message),
  error: (message: string) => useToastStore.getState().push("error", message),
  info: (message: string) => useToastStore.getState().push("info", message),
};

const KIND_STYLES: Record<ToastKind, string> = {
  success: "bg-card text-card-foreground",
  error: "bg-destructive text-destructive-foreground",
  info: "bg-primary text-primary-foreground",
};

const KIND_MARK: Record<ToastKind, string> = {
  success: "✓",
  error: "✕",
  info: "›",
};

function ToastRow({ item }: { item: ToastItem }) {
  return (
    <button
      onClick={() => useToastStore.getState().dismiss(item.id)}
      className={cn(
        "pointer-events-auto flex w-full max-w-sm cursor-pointer items-start gap-2 rounded-md border-2 border-border px-3 py-2 text-left text-xs font-bold uppercase tracking-wide shadow-brutal animate-od-slide-up",
        KIND_STYLES[item.kind],
      )}
      role="status"
    >
      <span aria-hidden>{KIND_MARK[item.kind]}</span>
      <span className="min-w-0 break-words">{item.message}</span>
    </button>
  );
}

export function Toaster() {
  const items = useToastStore((s) => s.items);
  return (
    <div className="pointer-events-none fixed right-4 bottom-4 z-50 flex flex-col gap-2">
      {items.map((item) => (
        <ToastRow key={item.id} item={item} />
      ))}
    </div>
  );
}
```

- [ ] **Step 2: Mount in MainApp**

In `src/App.tsx`: import `import { Toaster } from "@/components/ui/toast";` and render `<Toaster />` as the last child inside MainApp's root `<div>` (right after `{settings && !settings.onboarded && <Onboarding />}`). Do NOT mount it in `DockApp`.

- [ ] **Step 3: Verify build**

Run: `npm run build`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add src/components/ui/toast.tsx src/App.tsx
git commit -m "feat(ui): brutal-style toast primitive with success/error/info variants"
```

---

### Task 4: Confirm dialog primitive + replace window.confirm sites

**Files:**
- Create: `src/components/ui/confirm-dialog.tsx`
- Modify: `src/components/tabs/SnippetsTab.tsx:69`
- Modify: `src/components/tabs/HistoryTab.tsx:70,94`
- Modify: `src/components/tabs/HeatmapTab.tsx:95` (reset stats confirm)
- Modify: `src/App.tsx` (mount provider if needed)

**Interfaces:**
- Produces: `confirmDialog(opts: { title: string; description?: string; confirmLabel?: string; destructive?: boolean }): Promise<boolean>` — store-based, no context provider needed.
- Consumes: existing `Dialog` primitives in `src/components/ui/dialog.tsx`, `Button`.

- [ ] **Step 1: Read `src/components/ui/dialog.tsx`** to learn exported names (e.g. `Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription, DialogFooter`). Adjust imports below to match exactly.

- [ ] **Step 2: Create `src/components/ui/confirm-dialog.tsx`**

```tsx
import { create } from "zustand";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";

interface ConfirmOptions {
  title: string;
  description?: string;
  confirmLabel?: string;
  destructive?: boolean;
}

interface ConfirmState extends ConfirmOptions {
  open: boolean;
  resolve: ((ok: boolean) => void) | null;
}

export const useConfirmStore = create<ConfirmState>()(() => ({
  open: false,
  title: "",
  description: undefined,
  confirmLabel: "Confirm",
  destructive: false,
  resolve: null,
}));

export function confirmDialog(opts: ConfirmOptions): Promise<boolean> {
  return new Promise((resolve) => {
    useConfirmStore.setState({ ...opts, open: true, resolve });
  });
}

function settle(ok: boolean): void {
  const { resolve } = useConfirmStore.getState();
  useConfirmStore.setState({ open: false });
  resolve?.(ok);
}

export function ConfirmDialogHost() {
  const { open, title, description, confirmLabel, destructive } = useConfirmStore();
  return (
    <Dialog open={open} onOpenChange={(o) => !o && settle(false)}>
      <DialogContent className="max-w-sm">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          {description && <DialogDescription>{description}</DialogDescription>}
        </DialogHeader>
        <DialogFooter>
          <Button variant="outline" onClick={() => settle(false)}>
            Cancel
          </Button>
          <Button
            variant={destructive ? "destructive" : "default"}
            onClick={() => settle(true)}
          >
            {confirmLabel ?? (destructive ? "Delete" : "Confirm")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
```

- [ ] **Step 3: Mount host in App.tsx** next to `<Toaster />`: import and render `<ConfirmDialogHost />` in `MainApp` only.

- [ ] **Step 4: Make Button's destructive variant actually red**

In `src/components/ui/button.tsx`, replace the `destructive:` variant string (line ~20, currently identical to default/black) with:

```ts
        destructive:
          "bg-destructive text-destructive-foreground border-destructive shadow-[3px_3px_0_0_#000] hover:shadow-[1px_1px_0_0_#000] hover:translate-x-[1px] hover:translate-y-[1px] active:translate-x-[3px] active:translate-y-[3px] active:shadow-none",
```

- [ ] **Step 5: Replace the four window.confirm call sites**

Each site changes from sync-guard to await-confirm. Pattern shown for SnippetsTab `handleDelete` (apply analogously to the others):

```tsx
  const handleDelete = async (id: number, triggerName: string) => {
    const ok = await confirmDialog({
      title: "Delete snippet?",
      description: `“${triggerName}” will be removed permanently.`,
      confirmLabel: "Delete",
      destructive: true,
    });
    if (!ok) return;
    try {
      await api.removeSnippet(id);
      toast.success(`Deleted “${triggerName}”`);
      await useStore.getState().refreshAll();
    } catch (e) {
      toast.error(String(e));
    }
  };
```

Sites:
1. `SnippetsTab.tsx:69` — as above (uses `api.removeSnippet(id)`).
2. `HistoryTab.tsx:70` `handleDelete` — title `"Delete dictation?"`, description `"This entry will be removed permanently."`, body calls `api.deleteHistory(id)` then `refreshAll()`, plus `toast.success("Deleted")` / `toast.error(String(e))`.
3. `HistoryTab.tsx:94` `handleClearAll` — title `"Clear all history?"`, description `"Every dictation entry will be removed permanently. This cannot be undone."`, confirmLabel `"Clear all"`, body calls `api.clearHistory()` then `refreshAll()`, toasts likewise.
4. `HeatmapTab.tsx:95` reset stats — read the surrounding handler first; keep its API call(s) intact, wrap with `confirmDialog({ title: "Reset statistics?", description: "All word counts and streaks will be zeroed.", confirmLabel: "Reset", destructive: true })`, then toast on success/error.

Add imports for `confirmDialog` and `toast` where used.

- [ ] **Step 6: Verify**

Run: `grep -rn "window.confirm" src/`
Expected: no matches.
Run: `npm run build`
Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add src/components/ui/confirm-dialog.tsx src/components/ui/button.tsx src/components/tabs/SnippetsTab.tsx src/components/tabs/HistoryTab.tsx src/components/tabs/HeatmapTab.tsx src/App.tsx
git commit -m "feat(ui): styled confirm dialog replaces native window.confirm; destructive button variant goes red"
```

---

### Task 5: Textarea + Slider primitives, adopt Card container

**Files:**
- Create: `src/components/ui/textarea.tsx`
- Create: `src/components/ui/slider.tsx`
- Modify: `src/components/tabs/SnippetsTab.tsx:131-137,196-202`
- Modify: `src/components/tabs/DictionaryTab.tsx:90-96`
- Modify: `src/components/tabs/HistoryTab.tsx:167-172`
- Modify: `src/components/tabs/SettingsTab.tsx:326-335`
- Modify: `src/components/ui/card.tsx` (restyle to brutal card)
- Modify: ad-hoc card divs in tabs (see Step 5)

**Interfaces:**
- Produces: `<Textarea />` with same props as raw textarea + consistent styling; `<Slider value min max onChange />` themed range input; `<Card>` family used as standard bordered container.

- [ ] **Step 1: Create `src/components/ui/textarea.tsx`**

```tsx
import * as React from "react";
import { cn } from "@/lib/utils";

function Textarea({ className, ...props }: React.ComponentProps<"textarea">) {
  return (
    <textarea
      data-slot="textarea"
      className={cn(
        "w-full resize-y rounded-md border-2 border-input bg-transparent px-3 py-2 text-sm outline-none transition-colors placeholder:text-muted-foreground focus-visible:border-brand focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-40",
        className,
      )}
      {...props}
    />
  );
}

export { Textarea };
```

- [ ] **Step 2: Create `src/components/ui/slider.tsx`**

```tsx
import { cn } from "@/lib/utils";

interface SliderProps extends Omit<React.InputHTMLAttributes<HTMLInputElement>, "type" | "onChange"> {
  value: number;
  min?: number;
  max?: number;
  onChange: (value: number) => void;
}

/** Themed range input: track shows accent fill up to the thumb. */
function Slider({ value, min = 0, max = 100, onChange, className, ...props }: SliderProps) {
  const pct = ((value - min) / (max - min)) * 100;
  return (
    <input
      type="range"
      min={min}
      max={max}
      value={value}
      onChange={(e) => onChange(Number(e.target.value))}
      className={cn("h-2 w-full cursor-pointer appearance-none rounded-full border-2 border-border bg-secondary outline-none focus-visible:ring-2 focus-visible:ring-ring [&::-webkit-slider-thumb]:size-4 [&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:border-2 [&::-webkit-slider-thumb]:border-border [&::-webkit-slider-thumb]:bg-brand", className)}
      style={{
        background: `linear-gradient(to right, var(--brand) ${pct}%, var(--secondary) ${pct}%)`,
        ...props.style,
      }}
      {...props}
    />
  );
}

export { Slider };
```

(Add `import type * as React from "react"` at top.)

- [ ] **Step 3: Restyle ui/card.tsx as the standard brutal container**

Read the file first; then make Card's base classes:

```
"flex flex-col rounded-none border-2 border-border bg-card text-card-foreground shadow-brutal"
```

Keep CardHeader/CardTitle/CardContent/CardFooter exports unchanged structurally; give CardContent `p-4` default padding if it has none. If any tab already imports Card, verify nothing breaks.

- [ ] **Step 4: Replace all raw textareas**

Replace each raw `<textarea>` block with `<Textarea />`, dropping the duplicated class strings. E.g. SnippetsTab line 131:

```tsx
<Textarea
  value={text}
  onChange={(event) => setText(event.target.value)}
  placeholder="Template text that gets inserted when you say: “insert snippet <trigger>”…"
  rows={3}
/>
```

Same replacement at SnippetsTab:196 (edit row), DictionaryTab:90 (bulk paste), HistoryTab:167 (edit entry — also add `aria-label="Edit dictation text"`).

- [ ] **Step 5: Replace the SettingsTab volume slider**

SettingsTab lines 326–335 become:

```tsx
          <Slider
            id="audio-feedback-volume"
            className="flex-1"
            min={0}
            max={100}
            value={Math.round((settings?.audio_feedback_volume ?? 0.5) * 100)}
            onChange={(v) => handleVolumeChange(v / 100)}
          />
```

Import `Slider` from `@/components/ui/slider`.

- [ ] **Step 6: Adopt Card for ad-hoc containers**

Replace these ad-hoc wrappers with `<Card><CardContent className="flex flex-col gap-2 p-3">...</CardContent></Card>`:
- HomeTab ReadyStrip cards (HomeTab.tsx:83 and :116, currently `border-2 border-black bg-white p-3`)
- HomeTab LastResultPanel result row (:224) and LiveCaptions strip (:264 — keep `bg-primary text-primary-foreground` styling by passing className)
- SnippetsTab add-form panel (:110)
- HistoryTab table wrapper (:151, `border-2 border-black shadow-[6px_6px_0_0_#E8E8E8]` → `<Card>`)

Preserve any extra classes via `className` props.

- [ ] **Step 7: Verify**

Run: `grep -rn "<textarea" src/components/tabs/`
Expected: no matches.
Run: `grep -rn 'style={{ accentColor' src/`
Expected: no matches.
Run: `npm run build`
Expected: clean.

- [ ] **Step 8: Commit**

```bash
git add src/components/ui/textarea.tsx src/components/ui/slider.tsx src/components/ui/card.tsx src/components/tabs/
git commit -m "feat(ui): Textarea + Slider primitives, Card adopted as standard container"
```

---

### Task 6: Shared useRecording hook

**Files:**
- Create: `src/lib/useRecording.ts`
- Modify: `src/App.tsx:65-103` (RecordingButton)
- Modify: `src/components/tabs/HomeTab.tsx:163-210` (RecordButton)

**Interfaces:**
- Produces: `useRecording(): { recording: boolean; busy: boolean; toggle: () => Promise<void>; }` — errors surface via `toast.error`, stop sets `lastResult` when text returned.

- [ ] **Step 1: Create `src/lib/useRecording.ts`**

```ts
import { useState } from "react";
import * as api from "@/lib/api";
import { useStore } from "@/lib/store";
import { toast } from "@/components/ui/toast";

export function useRecording() {
  const recording = useStore((s) => s.recording);
  const [busy, setBusy] = useState(false);

  const toggle = async () => {
    if (busy) return;
    setBusy(true);
    try {
      if (recording) {
        try {
          const result = await api.stopRecording();
          if (result?.text) useStore.setState({ lastResult: result });
        } finally {
          useStore.getState().setRecording(false);
        }
      } else {
        try {
          await api.startRecording("dictate");
          useStore.getState().setRecording(true);
        } catch (e) {
          toast.error(`Could not start recording: ${String(e)}`);
        }
      }
    } finally {
      setBusy(false);
    }
  };

  return { recording, busy, toggle };
}
```

Note: stop failures still reset `recording` state (finally) but are surfaced by the caller adding `toast.error` on stop failure too — extend: capture stop error and toast it:

```ts
      if (recording) {
        try {
          const result = await api.stopRecording();
          if (result?.text) useStore.setState({ lastResult: result });
        } catch (e) {
          toast.error(`Stop failed: ${String(e)}`);
        } finally {
          useStore.getState().setRecording(false);
        }
      }
```

- [ ] **Step 2: Refactor RecordingButton in App.tsx**

Replace lines 65–103 with:

```tsx
function RecordingButton() {
  const { recording, toggle } = useRecording();

  return (
    <Button onClick={toggle} variant={recording ? "outline" : "default"} size="sm">
      {recording ? "■ STOP" : "● RECORD"}
    </Button>
  );
}
```

Remove now-unused `useState` import if applicable elsewhere; keep header layout otherwise unchanged.

- [ ] **Step 3: Refactor RecordButton in HomeTab.tsx**

Replace lines 163–210 with:

```tsx
function RecordButton() {
  const { recording, toggle } = useRecording();

  return (
    <div className="flex flex-col gap-2">
      <Button
        onClick={toggle}
        variant={recording ? "outline" : "default"}
        className={`h-16 w-full text-lg font-bold tracking-widest uppercase ${
          recording ? "animate-od-blink" : ""
        }`}
      >
        {recording ? "■ Stop" : "● Record"}
      </Button>
      <p className="text-xs text-muted-foreground">
        {recording
          ? "Recording — press the global hotkey to stop."
          : "Dictate — press Record or your global hotkey."}
      </p>
    </div>
  );
}
```

Add `import { useRecording } from "@/lib/useRecording";` and remove unused imports (`useState` may still be used by ReadyStrip).

- [ ] **Step 4: Verify**

Run: `npm run build`
Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add src/lib/useRecording.ts src/App.tsx src/components/tabs/HomeTab.tsx
git commit -m "refactor(ui): single useRecording hook replaces three divergent record implementations"
```

---

### Task 7: Feedback sweep — Snippets + History rows

**Files:**
- Modify: `src/components/tabs/SnippetsTab.tsx`
- Modify: `src/components/tabs/HistoryTab.tsx`

**Interfaces:**
- Consumes: `toast`, `Textarea` (Task 3/5), `confirmDialog` (Task 4).

- [ ] **Step 1: SnippetsTab — toasts replace banners**

- Delete `message`/`error` state variables and their two banner blocks (lines ~155–164).
- `handleAdd`: on success `toast.success(\`Added “${trigger.trim()}”\`)`; on catch `toast.error(String(e))`.
- `handleQuickCapture`: `toast.info("Filled from last dictation — give it a trigger name")`.
- `saveEdit`: success `toast.success("Snippet updated")`, error toast.
- `handleExport`: success `toast.info(\`Exported to ${path}\`)`; remove `exported` banner state but keep a `revealExport(path)` helper storing the last export path in state for the "Reveal" button (keep the button, rendered inline after Export). On reveal failure `toast.error(...)`.
- `handleImport`: success/error toasts (message text unchanged); reveal failure toast in `handleReveal`.
- Remove the old `setMessage(...)` calls accordingly.

- [ ] **Step 2: SnippetsTab — unified icon row actions**

Row actions cell (non-editing rows, lines ~227–239) becomes a horizontal icon group:

```tsx
                    <TableCell>
                      <div className="flex items-center justify-end gap-1">
                        <Button size="icon-sm" variant="ghost" title="Edit" onClick={() => beginEdit(entry)}>
                          <Pencil />
                        </Button>
                        <Button size="icon-sm" variant="ghost" title="Copy text" onClick={() => handleCopySnippet(entry.text)}>
                          <Copy />
                        </Button>
                        <Button size="icon-sm" variant="ghost" title="Delete" onClick={() => handleDelete(entry.id, entry.trigger)}>
                          <Trash2 />
                        </Button>
                      </div>
                    </TableCell>
```

Add `import { Copy, Pencil, Trash2 } from "lucide-react";`. Add:

```tsx
  const handleCopySnippet = async (text: string) => {
    try {
      await api.copyText(text);
      toast.success("Copied");
    } catch (e) {
      toast.error(String(e));
    }
  };
```

Edit-row Save/Cancel become a horizontal pair; Cancel uses `variant="ghost"`.

- [ ] **Step 3: HistoryTab — fix channels + toasts**

- Delete `exportError` banner usage for edit errors: `saveEdit` catches → `toast.error(String(e))` (fixes wrong-channel bug at line 89). Keep `exportError` only for export failures, rendered via toast too — delete the `exportError` state and banner entirely; `handleExport` toasts success (`toast.info(\`Exported — ${path}\`)`) / error.
- `handleCopy` → `toast.success("Copied")` on success, error toast on failure.
- `handleInsert` → `toast.success("Sent to clipboard")` / error toast.
- `handleReveal` failure → toast.
- Row action buttons (lines ~195–208) become the same horizontal ghost icon group as Step 2: Edit (Pencil), Re-insert (ClipboardPaste), Copy (Copy), Delete (Trash2, `variant="destructive"`), each with `title=` tooltips. Edit-row Cancel switches `variant="ghost"`→`variant="outline"`? No — unify Cancel as `variant="ghost"` everywhere (Snippets edit Cancel also ghost).
- Table wrapper keeps `<Card>` from Task 5.

- [ ] **Step 4: Verify**

Run: `npm run build`
Expected: clean.
Manual: copy/re-insert/delete show toasts; delete opens styled dialog.

- [ ] **Step 5: Commit**

```bash
git add src/components/tabs/SnippetsTab.tsx src/components/tabs/HistoryTab.tsx
git commit -m "feat(ui): toast feedback + unified icon row actions in Snippets and History"
```

---

### Task 8: Feedback sweep — Dictionary, Home, Settings, Activity

**Files:**
- Modify: `src/components/tabs/DictionaryTab.tsx`
- Modify: `src/components/tabs/HomeTab.tsx`
- Modify: `src/components/tabs/SettingsTab.tsx`
- Modify: `src/components/tabs/HeatmapTab.tsx`

- [ ] **Step 1: DictionaryTab — no more silent catches**

- Replace `message` state + banner with toasts: `handleAdd` success `toast.success(\`Added “${trimmed}”\`)`, catch `toast.error(String(e))`; `handleBulkAdd` success/error toasts (same wording); `handleImport` likewise; `handleRemove` success `toast.success(\`Removed “${w}”\`)` / error toast.
- The badge ×-button stays; on hover it uses `hover:bg-destructive hover:text-destructive-foreground` instead of black.

- [ ] **Step 2: HomeTab — mic select + undo feedback**

- `ReadyStrip.handleMicChange` (line ~57): catch → `toast.error(\`Microphone switch failed: ${String(e)}\`)`; success → `toast.success(\`Microphone: ${micLabel(name, mics)}\`)`.
- Remove the standalone error banner block (lines ~154–158) — download errors go through `toast.error(String(e))` in `handleDownload`.
- `LastResultPanel` Undo (lines ~229–242): catch → `toast.error(String(e))`; success keeps button label change.

- [ ] **Step 3: SettingsTab — no more silent catches**

- `handleMicChange` (SettingsTab.tsx:158): catch → `toast.error(\`Microphone switch failed: ${String(e)}\`)`.
- `handleLanguageChange` (SettingsTab.tsx:166): catch → `toast.error(\`Language change failed: ${String(e)}\`)`.
- `persistToggle` rollback branch (the inner catch at line 179): after restoring latest settings, add `toast.error("Could not save setting — reverted")`. Keep the rollback logic unchanged.

Note: Settings needs no loading skeleton — it renders optimistically via `settings?.` defaults, so there is no blank/loading state to cover. (Deliberate deviation from the design-dialogue list; the spec's "loading skeletons" requirement is satisfied by History + Activity in Task 9.)

- [ ] **Step 4: HeatmapTab — color presets + stat cards survive dark**

- Audit hardcoded hex backgrounds in stat cards / heatmap cells: heatmap intensity levels must derive from the chosen preset (they already do); ensure card containers use `bg-card text-card-foreground` (Task 5 conversion) rather than `bg-white`.
- Ensure the custom color picker `<label>` styling matches the shared Import-button pattern used in Snippets/Dictionary (or convert to a small `Button variant="outline"` wrapper — match whichever the file already does after Task 5).

- [ ] **Step 4: Verify**

Run: `grep -rn "catch {}" src/components/tabs/`
Expected: no matches.
Run: `npm run build`
Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add src/components/tabs/DictionaryTab.tsx src/components/tabs/HomeTab.tsx src/components/tabs/SettingsTab.tsx src/components/tabs/HeatmapTab.tsx
git commit -m "feat(ui): dictionary/home/settings/activity feedback — toasts on every mutation, no silent failures"
```

---

### Task 9: Loading skeletons + Activity empty state + nav polish

**Files:**
- Create: `src/components/ui/skeleton.tsx`
- Modify: `src/components/tabs/HistoryTab.tsx`
- Modify: `src/components/tabs/HeatmapTab.tsx`
- Modify: `src/App.tsx`

**Interfaces:**
- Produces: `<Skeleton className="..." />` pulsing block; store flag `hydrated` on `useStore`.

- [ ] **Step 1: Skeleton primitive**

```tsx
import { cn } from "@/lib/utils";

function Skeleton({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="skeleton"
      className={cn("animate-pulse rounded-md border border-border/50 bg-muted", className)}
      {...props}
    />
  );
}

export { Skeleton };
```

- [ ] **Step 2: Add `hydrated` flag to store**

In `src/lib/store.ts`: add `hydrated: boolean;` to the interface (after `settingsRevision`), initial `false`, and at the end of `refreshAll` (inside `refreshAll`, after `set({...})`) add `set({ hydrated: true });`.

- [ ] **Step 3: History loading skeleton**

Top of HistoryTab component:

```tsx
  const hydrated = useStore((s) => s.hydrated);
```

When `!hydrated`, render instead of the table block:

```tsx
        <Card>
          <CardContent className="flex flex-col gap-2 p-4">
            {[0, 1, 2, 3].map((i) => (
              <Skeleton key={i} className="h-10 w-full" />
            ))}
          </CardContent>
        </Card>
```

(guarded so the search/export toolbar still renders).

- [ ] **Step 4: Activity empty state + skeleton**

HeatmapTab: if `stats === null && !hydrated` show four `<Skeleton className="h-24" />` in place of stat cards; if `hydrated && totalWords === 0` (locate the actual totals field in the file), render above the heatmap an empty-state panel matching other tabs:

```tsx
        <div className="rounded-none border-2 border-dashed border-border p-6 text-center">
          <p className="text-sm font-bold uppercase tracking-wider">No activity yet</p>
          <p className="mt-1 text-sm text-muted-foreground">
            Word counts appear here after your first dictation — try the Home tab.
          </p>
        </div>
```

- [ ] **Step 5: Nav accent indicator + focus rings**

In App.tsx nav buttons: active button gets `relative` plus an indicator span rendered when `tab === t.id`:

```tsx
                {tab === t.id && (
                  <span className="absolute inset-y-0 left-0 w-1 bg-brand" aria-hidden />
                )}
```

(add `relative` to the button's className). Global focus ring is already token-driven via `--ring`; verify `:root` outline utility line (`outline-ring/50`) still applies.

- [ ] **Step 6: Verify**

Run: `npm run build`
Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add src/components/ui/skeleton.tsx src/lib/store.ts src/components/tabs/HistoryTab.tsx src/components/tabs/HeatmapTab.tsx src/App.tsx
git commit -m "feat(ui): loading skeletons, activity empty state, accent nav indicator"
```

---

### Task 10: Hardcoded-color audit + final gates

**Files:**
- Modify: remaining components with hardcoded `black`/`white`/hex utilities discovered by audit: `src/components/Onboarding.tsx`, `src/components/MicTest.tsx`, `src/components/ModelCard.tsx` (NOT DockButton.tsx).

- [ ] **Step 1: Audit**

Run: `grep -rn "border-black\|bg-black\|bg-white\|text-white\|text-black\|#E8E8E8" src/components/ src/App.tsx | grep -v DockButton | grep -v index.css`
Expected output: a list. For each hit outside intentional brand marks (the OD logo tile in the header keeps inverted styling), map mechanically:
- `border-black` → `border-border`
- `bg-black text-white` pairs → `bg-primary text-primary-foreground` (inverted emphasis blocks) or `bg-card text-card-foreground` (containers)
- `bg-white` containers → `bg-card`
- `text-white/NN` → opacity variants of the local foreground (e.g. `text-muted-foreground`)
- `shadow-[Npx_Npx_0_0_#000]` → `shadow-brutal` / `shadow-brutal-hover` / `shadow-brutal-none` (match the Npx offset: 3px→brutal, 1px→hover, 0→none)
- `shadow-[..._#E8E8E8]` → drop (use plain `shadow-brutal`)

- [ ] **Step 2: Apply mapping** to Onboarding.tsx, MicTest.tsx, ModelCard.tsx and any tab hits missed earlier.

- [ ] **Step 3: Final gates**

Run each; all must pass (no output):
```bash
grep -rn "window.confirm" src/
grep -rn "catch {}" src/components/tabs/
grep -rn "<textarea" src/components/tabs/
grep -rn "classList.add(\"dark\")" src/main.tsx
```

Run: `npm run build`
Expected: clean.

- [ ] **Step 4: Manual pass (both themes)**

Via `npm run tauri dev`, walk: Onboarding dialog, Home (record start/stop, mic switch, model download), Activity (presets, reset confirm), Dictionary (add/bulk/import/export/remove), Snippets (CRUD + import/export), History (search/edit/copy/re-insert/delete/clear-all/exports), Settings (every toggle, hotkey capture, volume slider, appearance switch). Confirm contrast in light and dark, toasts stack/dismiss, confirms are styled.

- [ ] **Step 5: Commit**

```bash
git add -A src/
git commit -m "polish(ui): token audit — no hardcoded blacks/whites outside brand marks"
```
