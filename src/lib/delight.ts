/**
 * Small, truthful acknowledgements of what dictation saved. The typing speed is
 * the same 40 words a minute that History uses for "× faster than typing".
 */
export const TYPING_WPM = 40;

export function wordCount(text: string): number {
  return text.trim().split(/\s+/).filter(Boolean).length;
}

function plural(n: number, unit: string): string {
  return `${n} ${unit}${n === 1 ? "" : "s"}`;
}

/** Seconds a dictation saved against typing it, or null when it saved little. */
export function secondsSaved(text: string, durationMs: number): number | null {
  const saved = (wordCount(text) / TYPING_WPM) * 60 - durationMs / 1000;
  return saved >= 3 ? saved : null;
}

export function formatSpan(seconds: number): string {
  if (seconds < 90) return plural(Math.round(seconds), "second");
  const minutes = Math.round(seconds / 60);
  if (minutes < 90) return plural(minutes, "minute");
  return plural(Math.round(minutes / 60), "hour");
}

/** Word totals worth marking. 1 stands for the first dictation. */
export const MILESTONES = [1, 1_000, 10_000, 50_000, 100_000, 250_000, 500_000, 1_000_000];

function reached(totalWords: number): number {
  return MILESTONES.filter((mark) => totalWords >= mark).pop() ?? 0;
}

/**
 * Decides whether a milestone is new. `stored` is the last mark already shown,
 * or null on a first run, where the current total becomes the baseline without
 * a celebration, so people upgrading are not congratulated for old work.
 */
export function nextMilestone(
  totalWords: number,
  stored: number | null,
): { show: number | null; store: number } {
  const mark = reached(totalWords);
  if (stored === null || mark <= stored) return { show: null, store: mark };
  return { show: mark, store: mark };
}

export function milestoneCopy(mark: number): { title: string; body: string } {
  if (mark === 1) {
    return {
      title: "Your first dictation landed",
      body: "Every dictation is kept in History, on this device only.",
    };
  }
  return {
    title: `${mark.toLocaleString()} words by voice`,
    body: `About ${formatSpan((mark / TYPING_WPM) * 60)} of typing at ${TYPING_WPM} words a minute.`,
  };
}
