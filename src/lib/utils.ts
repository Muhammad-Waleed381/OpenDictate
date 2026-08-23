import { clsx, type ClassValue } from "clsx"
import { twMerge } from "tailwind-merge"

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}

export function formatHotkey(hotkey: string): string {
  return hotkey
    .split("+")
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join("+");
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
