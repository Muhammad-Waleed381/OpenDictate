import { useMemo } from "react";
import { useStore } from "@/lib/store";
import * as api from "@/lib/api";

const DAY_MS = 86_400_000;

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

function levelFor(words: number): number {
  if (words <= 0) return 0;
  if (words <= 2) return 1;
  if (words <= 5) return 2;
  if (words <= 10) return 3;
  return 4;
}

const DEFAULT_COLOR = "#16a34a";

const PRESETS: { name: string; hex: string }[] = [
  { name: "Green", hex: "#16a34a" },
  { name: "Blue", hex: "#2563eb" },
  { name: "Violet", hex: "#7c3aed" },
  { name: "Amber", hex: "#d97706" },
  { name: "Rose", hex: "#e11d48" },
  { name: "Slate", hex: "#475569" },
];

function hexToRgb(hex: string): [number, number, number] | null {
  let h = hex.replace("#", "");
  if (h.length === 3) h = h.split("").map((c) => c + c).join("");
  if (h.length !== 6) return null;
  const n = parseInt(h, 16);
  if (Number.isNaN(n)) return null;
  return [(n >> 16) & 0xff, (n >> 8) & 0xff, n & 0xff];
}

function mix(hex: string, other: [number, number, number], t: number): string {
  const rgb = hexToRgb(hex);
  if (!rgb) return DEFAULT_COLOR;
  const out = rgb.map((v, i) => Math.round(v * (1 - t) + other[i] * t));
  return `#${out.map((v) => v.toString(16).padStart(2, "0")).join("")}`;
}

function shadesFor(hex: string): (string | null)[] {
  const white: [number, number, number] = [255, 255, 255];
  const black: [number, number, number] = [0, 0, 0];
  return [
    null,
    mix(hex, white, 0.7),
    mix(hex, white, 0.4),
    mix(hex, white, 0.1),
    mix(hex, black, 0.3),
  ];
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
}

export function HeatmapTab() {
  const stats = useStore((s) => s.stats);
  const settings = useStore((s) => s.settings);
  const color = settings?.heatmap_color ?? DEFAULT_COLOR;
  const cellColors = useMemo(() => shadesFor(color), [color]);

  const handleColor = async (hex: string) => {
    try {
      await api.setSettings({ heatmap_color: hex });
      await useStore.getState().refreshAll();
    } catch {
      // ignore transient save failures
    }
  };

  const handleReset = async () => {
    if (!window.confirm("Reset all word activity? This clears the heatmap and counters permanently.")) return;
    try {
      await api.resetWordStats();
      await useStore.getState().refreshAll();
    } catch {
      // ignore transient failures
    }
  };

  const { cells, monthLabels, weekLabels, legend } = useMemo(() => {
    const byDay = new Map<string, number>();
    for (const entry of stats?.daily ?? []) {
      byDay.set(entry.day, entry.words);
    }

    const today = startOfDay(new Date());
    const todayDow = (today.getDay() + 6) % 7; // Monday = 0
    const endMonday = new Date(today.getTime() - todayDow * DAY_MS);
    const startMonday = new Date(endMonday.getTime() - 52 * 7 * DAY_MS);

    const cells: HeatmapCell[] = [];
    const monthLabels: { col: number; label: string }[] = [];
    let lastMonth = -1;

    for (let col = 0; col < 53; col++) {
      const colStart = new Date(startMonday.getTime() + col * 7 * DAY_MS);
      const month = colStart.getMonth();
      if (month !== lastMonth) {
        lastMonth = month;
        monthLabels.push({ col, label: MONTH_LABELS[month] });
      }
      for (let row = 0; row < 7; row++) {
        const date = new Date(colStart.getTime() + row * DAY_MS);
        if (date.getTime() > today.getTime()) continue;
        const key = localDayKey(date);
        const words = byDay.get(key) ?? 0;
        cells.push({ date, key, words, level: levelFor(words) });
      }
    }

    const weekLabels = [
      { row: 1, label: "Mon" },
      { row: 3, label: "Wed" },
      { row: 5, label: "Fri" },
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

  const statCard =
    "flex flex-col gap-1 border-2 border-black bg-white p-4 shadow-[4px_4px_0_0_#E8E8E8]";

  const cellsByCol: HeatmapCell[][] = useMemo(() => {
    const cols: HeatmapCell[][] = Array.from({ length: 53 }, () => []);
    for (const cell of cells) {
      const idx = Math.floor(
        (cell.date.getTime() - new Date(cells[0]?.date ?? 0).getTime()) / (7 * DAY_MS)
      );
      cols[Math.max(0, idx)]?.push(cell);
    }
    return cols;
  }, [cells]);

  return (
    <div className="flex flex-col gap-6">
      <div className="grid grid-cols-2 gap-4 sm:grid-cols-4">
        <div className={statCard}>
          <span className="text-[10px] font-bold tracking-widest text-muted-foreground uppercase">
            Words transcribed
          </span>
          <span className="text-3xl font-black tabular-nums">
            {totals.total_words.toLocaleString()}
          </span>
        </div>
        <div className={statCard}>
          <span className="text-[10px] font-bold tracking-widest text-muted-foreground uppercase">
            Dictations
          </span>
          <span className="text-3xl font-black tabular-nums">
            {totals.total_sessions.toLocaleString()}
          </span>
        </div>
        <div className={statCard}>
          <span className="text-[10px] font-bold tracking-widest text-muted-foreground uppercase">
            Day streak
          </span>
          <span className="text-3xl font-black tabular-nums">
            {totals.streak_days}
            <span className="ml-1 text-sm font-bold text-muted-foreground">days</span>
          </span>
        </div>
        <div className={statCard}>
          <span className="text-[10px] font-bold tracking-widest text-muted-foreground uppercase">
            Best day
          </span>
          <span className="text-3xl font-black tabular-nums">
            {totals.best_words.toLocaleString()}
          </span>
          <span className="text-xs font-bold text-muted-foreground">
            {totals.best_words > 0 ? `words · ${bestLabel}` : "—"}
          </span>
        </div>
      </div>

      <div className="border-2 border-black bg-white p-5 shadow-[6px_6px_0_0_#E8E8E8]">
        <div className="mb-4 flex flex-wrap items-center gap-x-4 gap-y-3">
          <div className="flex items-baseline gap-2">
            <h3 className="text-sm font-black tracking-widest uppercase">
              Word activity
            </h3>
            <span className="text-xs font-bold text-muted-foreground">
              words transcribed per day · last 52 weeks
            </span>
          </div>
          <div className="ml-auto flex items-center gap-3">
            <button
              onClick={handleReset}
              className="cursor-pointer border-2 border-black/30 px-2 py-0.5 text-[10px] font-bold tracking-widest text-muted-foreground uppercase transition-colors hover:border-red-500 hover:text-red-500"
            >
              Reset stats
            </button>
            <div className="flex items-center gap-2">
            <span className="text-[10px] font-bold tracking-widest text-muted-foreground uppercase">
              Color
            </span>
            {PRESETS.map((p) => (
              <button
                key={p.hex}
                title={p.name}
                onClick={() => handleColor(p.hex)}
                className={`size-5 cursor-pointer border-2 ${
                  color === p.hex ? "border-black" : "border-black/20"
                }`}
                style={{ backgroundColor: p.hex }}
              />
            ))}
            <label
              title="Custom color"
              className="relative flex size-5 cursor-pointer items-center justify-center border-2 border-black/20 bg-white"
            >
              <input
                type="color"
                value={color}
                onChange={(e) => handleColor(e.target.value)}
                className="size-0 cursor-pointer opacity-0"
              />
              <span
                className="size-2.5 border border-black"
                style={{ backgroundColor: color }}
              />
            </label>
          </div>
        </div>
      </div>

        <div className="overflow-x-auto">
          <div className="inline-flex flex-col gap-1.5">
            <div className="flex">
              <div className="w-8 shrink-0" />
              <div className="relative h-4">
                {monthLabels.map((m) => (
                  <span
                    key={`${m.col}-${m.label}`}
                    className="absolute text-[10px] font-bold text-muted-foreground"
                    style={{ left: m.col * 13 }}
                  >
                    {m.label}
                  </span>
                ))}
              </div>
            </div>
            <div className="flex">
              <div className="flex w-8 shrink-0 flex-col">
                {weekLabels.map((w) => (
                  <span
                    key={w.row}
                    className="flex h-[13px] items-center text-[9px] font-bold text-muted-foreground"
                    style={{ marginTop: w.row === 1 ? 0 : 1 }}
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
                          title={`${cell.date.toLocaleDateString(undefined, {
                            weekday: "short",
                            year: "numeric",
                            month: "short",
                            day: "numeric",
                          })} — ${cell.words} word${cell.words === 1 ? "" : "s"}`}
                          className="size-[13px] border-2 border-black/5 bg-zinc-100"
                          style={bg ? { backgroundColor: bg } : undefined}
                        />
                      );
                    })}
                  </div>
                ))}
              </div>
            </div>
            <div className="flex items-center justify-end gap-1.5 text-[10px] font-bold text-muted-foreground">
              {legend.map((l) => {
                const bg = cellColors[l.level];
                return (
                  <span key={l.level} className="flex items-center gap-1">
                    {l.label && <span>{l.label}</span>}
                    <span
                      className="size-[11px] border-2 border-black/5 bg-zinc-100"
                      style={bg ? { backgroundColor: bg } : undefined}
                    />
                  </span>
                );
              })}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}