import { clsx, type ClassValue } from "clsx"
import { twMerge } from "tailwind-merge"

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}

export const IS_MAC =
  typeof navigator !== "undefined" &&
  /Mac|iPhone|iPad|iPod/.test(navigator.userAgent);

/**
 * Mirrors `default_hotkey()` in src-tauri/src/state.rs. Only a fallback for
 * render-before-settings-load; the backend remains the source of truth.
 */
export const DEFAULT_HOTKEY = IS_MAC ? "cmd+shift+space" : "ctrl+alt+space";

const MAC_MODIFIER_SYMBOLS: Record<string, string> = {
  cmd: "\u2318",
  command: "\u2318",
  super: "\u2318",
  meta: "\u2318",
  shift: "\u21e7",
  alt: "\u2325",
  option: "\u2325",
  ctrl: "\u2303",
  control: "\u2303",
};

/** Returns the modifier of a `double:<modifier>` shortcut, or null. */
export function parseDoubleTap(hotkey: string): string | null {
  const t = hotkey.trim().toLowerCase();
  return t.startsWith("double:") ? t.slice("double:".length) : null;
}

/** Gestures the backend can watch for. macOS only — see doubletap.rs. */
export const DOUBLE_TAP_OPTIONS = [
  { value: "double:fn", label: "Fn Fn" },
  { value: "double:cmd", label: "\u2318 \u2318" },
  { value: "double:ctrl", label: "\u2303 \u2303" },
  { value: "double:alt", label: "\u2325 \u2325" },
  { value: "double:shift", label: "\u21e7 \u21e7" },
] as const;

const capitalize = (part: string) =>
  part.charAt(0).toUpperCase() + part.slice(1);

/**
 * macOS writes shortcuts as glyphs run together (⌘⇧Space); every other
 * platform spells them out joined by "+".
 */
export function formatHotkey(hotkey: string): string {
  const gesture = parseDoubleTap(hotkey);
  if (gesture) {
    const glyph =
      MAC_MODIFIER_SYMBOLS[gesture] ?? capitalize(gesture === "fn" ? "fn" : gesture);
    return `${glyph} ${glyph}`;
  }
  const parts = hotkey
    .split("+")
    .map((part) => part.trim())
    .filter(Boolean);
  if (!IS_MAC) return parts.map(capitalize).join("+");
  return parts
    .map((part) => MAC_MODIFIER_SYMBOLS[part.toLowerCase()] ?? capitalize(part))
    .join("");
}

/**
 * Returns the trailing `maxChars` of `text`, snapped forward to a word
 * boundary, prefixed with an ellipsis. Live-caption surfaces are narrow and
 * must show what is being said NOW, not the sentence's opening words.
 */
export function tailForDisplay(text: string, maxChars: number): string {
  if (text.length <= maxChars) return text;
  let cut = text.slice(-maxChars);
  const firstSpace = cut.indexOf(" ");
  if (firstSpace !== -1 && firstSpace < maxChars * 0.35) {
    cut = cut.slice(firstSpace + 1);
  }
  return `…${cut}`;
}
