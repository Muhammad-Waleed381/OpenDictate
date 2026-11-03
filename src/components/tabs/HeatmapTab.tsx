import { useMemo, useState } from "react";
import { useStore } from "@/lib/store";
import * as api from "@/lib/api";
import { toast } from "@/components/ui/toast";
import { confirmDialog } from "@/components/ui/confirm-dialog";
import { Card } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { calculateAggregateSpeed, DEFAULT_HOTKEY, formatHotkey } from "@/lib/utils";

const CELL_PX = 13;
const GAP_PX = 3;
const PITCH_PX = CELL_PX + GAP_PX; // column/row stride of the grid

function localDayKey(d: Date): string {
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}

function startOfDay(d: Date): Date {
  const c = new Date(d);
  c.setHours(0, 0, 0, 0);
  return c;
}

/// Calendar-safe day arithmetic: setDate handles DST transitions correctly,
/// whereas millisecond math (`d.getTime() + n * 86_400_000`) shifts a day
/// across a fall-back change.
function addDays(d: Date, days: number): Date {
  const c = new Date(d);
  c.setDate(c.getDate() + days);
  return c;
}

function levelFor(words: number): number {
  if (words <= 0) return 0;
  if (words <= 2) return 1;
  if (words <= 5) return 2;
  if (words <= 10) return 3;
  return 4;
}

const DEFAULT_COLOR = "#562ff7";

/** The year is wider than a small window; the recent weeks are what people look for. */
const scrollToNewest = (element: HTMLDivElement | null) => {
  if (element) element.scrollLeft = element.scrollWidth;
};

const PRESETS: { name: string; hex: string }[] = [
  { name: "Galactic violet", hex: "#562ff7" },
  { name: "Ocean", hex: "#0e7fa8" },
  { name: "Pine", hex: "#1f7a6c" },
  { name: "Amber", hex: "#c98a1b" },
  { name: "Bubblegum pink", hex: "#f354d0" },
  { name: "Slate", hex: "#5a5370" },
];

function hexToRgb(hex: string): [number, number, number] | null {
  let h = hex.replace("#", "");
  if (h.length === 3) h = h.split("").map((c) => c + c).join("");
  if (h.length !== 6) return null;
  const n = parseInt(h, 16);
  if (Number.isNaN(n)) return null;
  return [(n >> 16) & 0xff, (n >> 8) & 0xff, n & 0xff];
}

/** Shades fade toward the card, so more activity reads stronger in both themes. */
function shadesFor(hex: string): (string | null)[] {
  const base = hexToRgb(hex) ? hex : DEFAULT_COLOR;
  return [null, 30, 52, 76, 100].map((percent) =>
    percent === null ? null : `color-mix(in srgb, ${base} ${percent}%, var(--card))`,
  );
}

const MONTH_LABELS = [
  "Jan", "Feb", "Mar", "Apr", "May", "Jun",
  "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

interface HeatmapCell {
  date: Date;
  key: string;
  words: number;
  level: number;
  col: number;
}

export function HeatmapTab() {
  const stats = useStore((s) => s.stats);
  const history = useStore((s) => s.history);
  const hydrated = useStore((s) => s.hydrated);
  const hotkey = formatHotkey(useStore((s) => s.settings?.hotkey) ?? DEFAULT_HOTKEY);
  const settings = useStore((s) => s.settings);
  // "#16a34a" was the previous default, not a choice the user made.
  const saved = settings?.heatmap_color;
  const color = !saved || saved === "#16a34a" ? DEFAULT_COLOR : saved;
  const cellColors = useMemo(() => shadesFor(color), [color]);
  const speedStats = useMemo(() => calculateAggregateSpeed(history), [history]);
  const [hoveredCell, setHoveredCell] = useState<{
    cell: HeatmapCell;
    rect: DOMRect;
  } | null>(null);

  const handleColor = async (hex: string) => {
    try {
      await api.setSettings({ heatmap_color: hex });
      await useStore.getState().refreshAll();
    } catch (e) {
      toast.error(`Could not save color: ${String(e)}`);
    }
  };

  const handleReset = async () => {
    const ok = await confirmDialog({
      title: "Reset statistics?",
      description: "All word counts and streaks will be zeroed.",
      confirmLabel: "Reset",
      destructive: true,
    });
    if (!ok) return;
    try {
      await api.resetWordStats();
      toast.success("Statistics reset");
      await useStore.getState().refreshAll();
    } catch (e) {
      toast.error(String(e));
    }
  };

  const { cells, monthLabels, weekLabels, legend } = useMemo(() => {
    const byDay = new Map<string, number>();
    for (const entry of stats?.daily ?? []) {
      byDay.set(entry.day, entry.words);
    }

    const today = startOfDay(new Date());
    const todayDow = (today.getDay() + 6) % 7; // Monday = 0
    const endMonday = addDays(today, -todayDow);
    const startMonday = addDays(endMonday, -52 * 7);

    const cells: HeatmapCell[] = [];
    const monthLabels: { col: number; label: string }[] = [];
    let lastMonth = -1;
    // A label is ~26px wide; skip one when it would collide with the
    // previously drawn label (short months, DST shifts).
    const MIN_LABEL_GAP_PX = 34;

    for (let col = 0; col < 53; col++) {
      const colStart = addDays(startMonday, col * 7);
      const month = colStart.getMonth();
      if (month !== lastMonth) {
        lastMonth = month;
        const prev = monthLabels[monthLabels.length - 1];
        if (!prev || (col - prev.col) * PITCH_PX >= MIN_LABEL_GAP_PX) {
          monthLabels.push({ col, label: MONTH_LABELS[month] });
        }
      }
      for (let row = 0; row < 7; row++) {
        const date = addDays(colStart, row);
        if (date.getTime() > today.getTime()) continue;
        const key = localDayKey(date);
        const words = byDay.get(key) ?? 0;
        cells.push({ date, key, words, level: levelFor(words), col });
      }
    }

    const weekLabels = [
      { row: 0, label: "Mon" },
      { row: 1, label: "Tue" },
      { row: 2, label: "Wed" },
      { row: 3, label: "Thu" },
      { row: 4, label: "Fri" },
      { row: 5, label: "Sat" },
      { row: 6, label: "Sun" },
    ];

    const legend = [
      { level: 0, label: "Less" },
      { level: 1, label: "" },
      { level: 2, label: "" },
      { level: 3, label: "" },
      { level: 4, label: "More" },
    ];

    return { cells, monthLabels, weekLabels, legend };
  }, [stats]);

  const totals = stats ?? {
    total_words: 0,
    total_sessions: 0,
    streak_days: 0,
    best_day: null,
    best_words: 0,
  };

  const bestLabel = totals.best_day
    ? new Date(`${totals.best_day}T00:00:00`).toLocaleDateString(undefined, {
        month: "short",
        day: "numeric",
      })
    : "—";

  const statCard = "gap-1 p-4";

  const cellsByCol: HeatmapCell[][] = useMemo(() => {
    const cols: HeatmapCell[][] = Array.from({ length: 53 }, () => []);
    for (const cell of cells) {
      cols[Math.max(0, cell.col)]?.push(cell);
    }
    return cols;
  }, [cells]);

  return (
    <div className="flex flex-col gap-6">
      {stats === null && !hydrated ? (
        <div className="grid grid-cols-2 gap-4 sm:grid-cols-3 lg:grid-cols-5">
          {[0, 1, 2, 3, 4].map((i) => (
            <Skeleton key={i} className="h-24" />
          ))}
        </div>
      ) : (
        <div className="grid grid-cols-6 gap-3 lg:grid-cols-5">
          <Card size="sm" className={`${statCard} col-span-2 lg:col-span-1`}>
            <span className="text-xs text-muted-foreground">
              Words transcribed
            </span>
            <span className="text-2xl font-semibold tabular-nums">
              {totals.total_words.toLocaleString()}
            </span>
          </Card>
          <Card size="sm" className={`${statCard} col-span-2 lg:col-span-1`}>
            <span className="text-xs text-muted-foreground">
              Dictations
            </span>
            <span className="text-2xl font-semibold tabular-nums">
              {totals.total_sessions.toLocaleString()}
            </span>
          </Card>
          <Card size="sm" className={`${statCard} col-span-2 lg:col-span-1`}>
            <span className="text-xs text-muted-foreground">
              Day streak
            </span>
            <span className="text-2xl font-semibold tabular-nums">
              {totals.streak_days}
              <span className="ml-1 text-sm font-semibold text-muted-foreground">days</span>
            </span>
          </Card>
          <Card size="sm" className={`${statCard} col-span-3 lg:col-span-1`}>
            <span className="text-xs text-muted-foreground">
              Best day
            </span>
            <span className="text-2xl font-semibold tabular-nums">
              {totals.best_words.toLocaleString()}
            </span>
            <span className="text-xs font-semibold text-muted-foreground">
              {totals.best_words > 0 ? `words · ${bestLabel}` : "—"}
            </span>
          </Card>
          <Card size="sm" className={`${statCard} col-span-3 lg:col-span-1`}>
            <span className="text-xs text-muted-foreground">
              Average speed
            </span>
            <span className="text-2xl font-semibold tabular-nums">
              {speedStats ? (
                <>
                  {speedStats.avgWpm}
                  <span className="ml-1 text-sm font-semibold text-muted-foreground">WPM</span>
                </>
              ) : (
                "—"
              )}
            </span>
            <span className="text-xs font-semibold text-muted-foreground">
              {speedStats ? `${speedStats.multiplier}× typing speed` : "—"}
            </span>
          </Card>
        </div>
      )}

      {hydrated && totals.total_words === 0 && (
        <div className="rounded-lg border border-dashed border-border p-6 text-center">
          <p className="text-sm font-semibold">No activity yet</p>
          <p className="mt-1 text-sm text-muted-foreground">
            Word counts appear here after your first dictation. Press {hotkey} in any app to start.
          </p>
        </div>
      )}

      <Card className="p-5">
        <div className="mb-4 flex flex-wrap items-center gap-x-4 gap-y-3">
          <div className="flex items-baseline gap-2">
            <h3 className="text-sm font-semibold ">
              Word activity
            </h3>
            <span className="text-xs font-semibold text-muted-foreground">
              words transcribed per day · last 52 weeks
            </span>
          </div>
          <div className="ml-auto flex items-center gap-3">
            <button
              onClick={handleReset}
              className="cursor-pointer rounded-sm border border-input px-2 py-0.5 text-xs text-muted-foreground transition-colors hover:border-destructive hover:text-destructive"
            >
              Reset stats
            </button>
            <div className="flex items-center gap-2">
            <span className="text-xs text-muted-foreground">
              Color
            </span>
            {PRESETS.map((p) => (
              <button
                key={p.hex}
                title={p.name}
                onClick={() => handleColor(p.hex)}
                aria-pressed={color === p.hex}
                className={`size-5 cursor-pointer rounded-full ${
                  color === p.hex ? "ring-2 ring-foreground ring-offset-2 ring-offset-card" : ""
                }`}
                style={{ backgroundColor: p.hex }}
              />
            ))}
            <label
              title="Custom color"
              className="relative flex size-5 cursor-pointer items-center justify-center rounded-full border border-input bg-card"
            >
              <input
                type="color"
                value={color}
                onChange={(e) => handleColor(e.target.value)}
                className="size-0 cursor-pointer opacity-0"
              />
              <span
                  className="size-2.5 rounded-full"
                style={{ backgroundColor: color }}
              />
            </label>
          </div>
        </div>
      </div>

        <div className="overflow-x-auto" ref={scrollToNewest}>
          <div className="inline-flex flex-col gap-1.5">
            <div className="flex">
              <div className="w-8 shrink-0" />
              <div className="relative h-4">
                {monthLabels.map((m) => (
                  <span
                    key={`${m.col}-${m.label}`}
                    className="absolute text-[11px] font-semibold text-muted-foreground"
                    style={{ left: m.col * PITCH_PX }}
                  >
                    {m.label}
                  </span>
                ))}
              </div>
            </div>
            <div className="flex">
              <div
                className="sticky left-0 z-10 w-8 shrink-0 bg-card"
                style={{ height: 7 * PITCH_PX - GAP_PX }}
              >
                {weekLabels.map((w) => (
                  <span
                    key={w.row}
                    className="absolute flex h-[13px] items-center text-[11px] leading-[13px] font-semibold text-muted-foreground"
                    style={{ top: w.row * PITCH_PX }}
                  >
                    {w.label}
                  </span>
                ))}
              </div>
              <div className="flex gap-[3px]">
                {cellsByCol.map((col, i) => (
                  <div key={i} className="flex flex-col gap-[3px]">
                    {col.map((cell) => {
                      const bg = cellColors[cell.level];
                      return (
                        <span
                          key={cell.key}
                          onMouseEnter={(e) => {
                            const rect = e.currentTarget.getBoundingClientRect();
                            setHoveredCell({ cell, rect });
                          }}
                          onMouseLeave={() => setHoveredCell(null)}
                          className="size-[13px] cursor-pointer rounded-[3px] bg-muted transition-transform hover:scale-125"
                          style={bg ? { backgroundColor: bg } : undefined}
                        />
                      );
                    })}
                  </div>
                ))}
              </div>
            </div>
            <div className="flex items-center justify-end gap-1.5 text-[11px] font-semibold text-muted-foreground">
              {legend.map((l) => {
                const bg = cellColors[l.level];
                return (
                  <span key={l.level} className="flex items-center gap-1">
                    {l.label && <span>{l.label}</span>}
                    <span
                      className="size-[11px] rounded-[3px] bg-muted"
                      style={bg ? { backgroundColor: bg } : undefined}
                    />
                  </span>
                );
              })}
            </div>
          </div>
        </div>

        {hoveredCell && (
          <div
            className="pointer-events-none fixed z-50 -translate-x-1/2 -translate-y-full transform rounded border border-border bg-popover px-2.5 py-1.5 text-xs text-popover-foreground animate-in fade-in-0 zoom-in-95 duration-100"
            style={{
              left: `${hoveredCell.rect.left + hoveredCell.rect.width / 2}px`,
              top: `${hoveredCell.rect.top - 8}px`,
            }}
          >
            <div className="font-semibold">
              {hoveredCell.cell.words.toLocaleString()}{" "}
              {hoveredCell.cell.words === 1 ? "word" : "words"}
            </div>
            <div className="text-[11px] text-muted-foreground">
              {hoveredCell.cell.date.toLocaleDateString(undefined, {
                weekday: "short",
                year: "numeric",
                month: "short",
                day: "numeric",
              })}
            </div>
          </div>
        )}
      </Card>
    </div>
  );
}
