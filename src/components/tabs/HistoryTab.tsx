import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useStore } from "@/lib/store";
import * as api from "@/lib/api";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { toast } from "@/components/ui/toast";
import { confirmDialog } from "@/components/ui/confirm-dialog";
import { ClipboardPaste, Copy, Pencil, Trash2 } from "lucide-react";
import { calculateWpm, calculateAggregateSpeed, cn, DEFAULT_HOTKEY, formatHotkey, wordsWorthLearning } from "@/lib/utils";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";

// Building a formatter is the slow part of toLocaleString, so every row shares
// one. The options match toLocaleString's default output.
const DATE_FORMAT = new Intl.DateTimeFormat(undefined, {
  year: "numeric",
  month: "numeric",
  day: "numeric",
  hour: "numeric",
  minute: "2-digit",
  second: "2-digit",
});

function formatDate(value: string): string {
  const num = Number(value);
  const date = new Date(Number.isFinite(num) && num < 1e12 ? num * 1000 : value);
  if (Number.isNaN(date.getTime())) return value;
  return DATE_FORMAT.format(date);
}

function formatDuration(ms: number): string {
  if (!ms || ms <= 0) return "< 1s";
  const sec = ms / 1000;
  if (sec < 60) return `${sec.toFixed(1)}s`;
  const mins = Math.floor(sec / 60);
  const remSec = Math.round(sec % 60);
  return `${mins}m ${remSec}s`;
}

/** The backend returns history in pages of this size. */
const PAGE = 500;
/** Rows drawn at first and added each time the list end scrolls into view. */
const RENDER_STEP = 50;

interface RowProps {
  entry: api.HistoryEntry;
  selected: boolean;
  expanded: boolean;
  editing: boolean;
  editText?: string;
  onEditText?: (text: string) => void;
  onSave?: () => void;
  onCancel?: () => void;
  onToggleSelect: (id: number) => void;
  onToggleExpand: (id: number) => void;
  onBeginEdit: (entry: api.HistoryEntry) => void;
  onInsert: (text: string) => void;
  onCopy: (text: string) => void;
  onDelete: (id: number) => void;
}

/** Redraws only when its own entry or state changes, not when another row does. */
const HistoryRow = memo(function HistoryRow({
  entry,
  selected,
  expanded,
  editing,
  editText,
  onEditText,
  onSave,
  onCancel,
  onToggleSelect,
  onToggleExpand,
  onBeginEdit,
  onInsert,
  onCopy,
  onDelete,
}: RowProps) {
  const wpm = calculateWpm(entry.text, entry.duration_ms);
  return (
    <TableRow className={selected ? "bg-muted/40" : ""}>
      <TableCell>
        <input
          type="checkbox"
          className="size-4 cursor-pointer accent-primary"
          checked={selected}
          onChange={() => onToggleSelect(entry.id)}
          aria-label={`Select dictation ${entry.id}`}
        />
      </TableCell>
      <TableCell className="w-full max-w-0">
        {editing ? (
          <div className="flex min-w-[220px] flex-col gap-2">
            <Textarea
              value={editText}
              onChange={(event) => onEditText?.(event.target.value)}
              rows={3}
              aria-label="Edit dictation text"
            />
            <div className="flex gap-1">
              <Button size="sm" onClick={onSave}>Save</Button>
              <Button size="sm" variant="ghost" onClick={onCancel}>Cancel</Button>
            </div>
          </div>
        ) : (
          <>
            <button
              className={`block w-full cursor-pointer text-left ${expanded ? "whitespace-pre-wrap" : "truncate"}`}
              onClick={() => onToggleExpand(entry.id)}
              title="Click to expand"
            >
              {entry.text}
            </button>
            <p className="mt-0.5 truncate text-xs text-muted-foreground tabular-nums">
              {[formatDate(entry.created_at), formatDuration(entry.duration_ms), entry.source].filter(Boolean).join(" · ")}
            </p>
          </>
        )}
      </TableCell>
      <TableCell className="text-right text-[13px] tabular-nums whitespace-nowrap">
        {wpm ? (
          <span title={`${(wpm / 40).toFixed(1)}× faster than typing`}>{wpm} WPM</span>
        ) : (
          <span className="text-muted-foreground">—</span>
        )}
      </TableCell>
      <TableCell className="text-right">
        <div className="flex items-center justify-end gap-1">
          <Button size="icon-sm" variant="ghost" title="Edit" onClick={() => onBeginEdit(entry)}>
            <Pencil />
          </Button>
          <Button size="icon-sm" variant="ghost" title="Re-insert: returns to the previous application" onClick={() => onInsert(entry.text)}>
            <ClipboardPaste />
          </Button>
          <Button size="icon-sm" variant="ghost" title="Copy" onClick={() => onCopy(entry.text)}>
            <Copy />
          </Button>
          <Button size="icon-sm" variant="ghost" className="hover:text-destructive" title="Delete" onClick={() => onDelete(entry.id)}>
            <Trash2 />
          </Button>
        </div>
      </TableCell>
    </TableRow>
  );
});

function LearnWords({
  words,
  onDone,
}: {
  words: string[];
  onDone: () => void;
}) {
  const [picked, setPicked] = useState<Set<string>>(() => new Set(words));
  const [saving, setSaving] = useState(false);

  const toggle = (word: string) =>
    setPicked((prev) => {
      const next = new Set(prev);
      if (next.has(word)) next.delete(word);
      else next.add(word);
      return next;
    });

  const add = async () => {
    setSaving(true);
    try {
      for (const word of picked) await api.addDictionaryWord(word);
      toast.success(
        picked.size === 1 ? `Added "${[...picked][0]}" to your Dictionary` : `Added ${picked.size} words to your Dictionary`,
      );
      await useStore.getState().refreshAll();
      onDone();
    } catch (e) {
      toast.error(String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <section
      aria-label="Add corrected words to the Dictionary"
      className="animate-od-slide-up flex flex-col gap-3 rounded-lg border border-border bg-card px-4 py-3"
    >
      <div className="flex flex-col gap-0.5">
        <h2 className="text-sm font-medium">Add your corrections to the Dictionary?</h2>
        <p className="text-xs text-muted-foreground">
          Dictionary words are spelled your way in future dictations.
        </p>
      </div>
      <div className="flex flex-wrap gap-1.5">
        {words.map((word) => (
          <button
            key={word}
            type="button"
            aria-pressed={picked.has(word)}
            onClick={() => toggle(word)}
            className={cn(
              "cursor-pointer rounded-md border px-2 py-1 text-[13px] transition-colors",
              picked.has(word)
                ? "border-transparent bg-positive-soft font-medium text-positive"
                : "border-input text-muted-foreground hover:text-foreground",
            )}
          >
            {word}
          </button>
        ))}
      </div>
      <div className="flex justify-end gap-2">
        <Button size="sm" variant="ghost" onClick={onDone}>
          Not now
        </Button>
        <Button size="sm" disabled={picked.size === 0 || saving} onClick={add}>
          {picked.size === 1 ? "Add 1 word" : `Add ${picked.size} words`}
        </Button>
      </div>
    </section>
  );
}

export function HistoryTab() {
  const history = useStore((s) => s.history);
  const hydrated = useStore((s) => s.hydrated);
  const dictionary = useStore((s) => s.dictionary);
  const hotkey = formatHotkey(useStore((s) => s.settings?.hotkey) ?? DEFAULT_HOTKEY);
  const [query, setQuery] = useState("");
  const [older, setOlder] = useState<api.HistoryEntry[]>([]);
  const [olderDone, setOlderDone] = useState(false);
  const [loadingOlder, setLoadingOlder] = useState(false);
  const [results, setResults] = useState<{ query: string; entries: api.HistoryEntry[] } | null>(null);
  const [searchRun, setSearchRun] = useState(0);
  const [learn, setLearn] = useState<string[] | null>(null);
  const [exported, setExported] = useState<string | null>(null);
  const [expandedId, setExpandedId] = useState<number | null>(null);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [editText, setEditText] = useState("");
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set());

  // New dictations push the newest page past older pages already shown, which
  // would leave a gap between the two, so the older pages start over.
  const newestPageEnd = history.length ? history[history.length - 1].id : 0;
  const lastPageEnd = useRef(newestPageEnd);
  useEffect(() => {
    if (newestPageEnd > lastPageEnd.current) {
      setOlder([]);
      setOlderDone(false);
    }
    lastPageEnd.current = newestPageEnd;
  }, [newestPageEnd]);

  const loaded = useMemo(() => {
    const ids = new Set(history.map((entry) => entry.id));
    return [...history, ...older.filter((entry) => !ids.has(entry.id))];
  }, [history, older]);

  const trimmedQuery = query.trim();
  // The newest page and any older pages loaded are searched at once; the
  // database search over every entry replaces that when it answers.
  useEffect(() => {
    if (!trimmedQuery) {
      setResults(null);
      return;
    }
    let current = true;
    const timer = window.setTimeout(() => {
      api
        .searchHistory(trimmedQuery)
        .then((entries) => {
          if (current) setResults({ query: trimmedQuery, entries });
        })
        .catch(() => {});
    }, 200);
    return () => {
      current = false;
      window.clearTimeout(timer);
    };
  }, [trimmedQuery, searchRun]);

  const filtered = useMemo(() => {
    if (!trimmedQuery) return loaded;
    if (results?.query === trimmedQuery) return results.entries;
    const q = trimmedQuery.toLowerCase();
    return loaded.filter((entry) => entry.text.toLowerCase().includes(q));
  }, [loaded, trimmedQuery, results]);

  const canShowOlder = !trimmedQuery && history.length >= PAGE && !olderDone;
  const showOlder = async () => {
    const oldest = loaded[loaded.length - 1];
    if (!oldest) return;
    setLoadingOlder(true);
    try {
      const page = await api.getHistoryBefore(oldest.id);
      setOlder((prev) => [...prev, ...page]);
      if (page.length < PAGE) setOlderDone(true);
    } catch (e) {
      toast.error(String(e));
    } finally {
      setLoadingOlder(false);
    }
  };

  // Keeps entries shown outside the newest page in step with an edit or delete.
  const afterChange = useCallback((change: (entries: api.HistoryEntry[]) => api.HistoryEntry[]) => {
    setOlder(change);
    setSearchRun((n) => n + 1);
  }, []);

  const [visibleCount, setVisibleCount] = useState(RENDER_STEP);
  useEffect(() => setVisibleCount(RENDER_STEP), [trimmedQuery]);
  const visible = filtered.slice(0, visibleCount);
  const moreToDraw = visibleCount < filtered.length;
  const listEnd = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    const end = listEnd.current;
    if (!end || !moreToDraw || typeof IntersectionObserver === "undefined") return;
    const observer = new IntersectionObserver(
      (seen) => {
        if (seen.some((item) => item.isIntersecting)) setVisibleCount((count) => count + RENDER_STEP);
      },
      { rootMargin: "800px 0px" },
    );
    observer.observe(end);
    return () => observer.disconnect();
  }, [moreToDraw, visibleCount]);

  const speedStats = useMemo(() => calculateAggregateSpeed(history), [history]);

  const selectedVisibleCount = filtered.filter((entry) => selectedIds.has(entry.id)).length;
  const allVisibleSelected = filtered.length > 0 && selectedVisibleCount === filtered.length;
  const someVisibleSelected = selectedVisibleCount > 0 && !allVisibleSelected;
  const toggleSelectAll = () => {
    setSelectedIds((previous) => {
      const next = new Set(previous);
      for (const entry of filtered) {
        if (allVisibleSelected) next.delete(entry.id);
        else next.add(entry.id);
      }
      return next;
    });
  };

  const toggleSelectOne = useCallback((id: number) => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  const handleBulkDelete = async () => {
    const count = selectedIds.size;
    if (count === 0) return;
    const visibleIds = new Set(filtered.map((entry) => entry.id));
    const hiddenSelected = [...selectedIds].filter((id) => !visibleIds.has(id)).length;
    const ok = await confirmDialog({
      title: `Delete ${count} dictations?`,
      description: `The selected ${count} entries will be permanently removed.${
        hiddenSelected > 0 ? ` ${hiddenSelected} of them are hidden by your search.` : ""
      }`,
      confirmLabel: `Delete (${count})`,
      destructive: true,
    });
    if (!ok) return;
    try {
      for (const id of selectedIds) {
        await api.deleteHistory(id);
      }
      const deleted = new Set(selectedIds);
      setSelectedIds(new Set());
      afterChange((entries) => entries.filter((entry) => !deleted.has(entry.id)));
      toast.success(`Deleted ${count} entries`);
      await useStore.getState().refreshAll();
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleExport = async (kind: api.ExportKind) => {
    try {
      const path = await api.exportHistory(kind);
      setExported(path);
      toast.info(`Exported to ${path}`);
    } catch (e) {
      setExported(null);
      toast.error(String(e));
    }
  };

  const handleReveal = async () => {
    if (!exported) return;
    try {
      await revealItemInDir(exported);
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleCopy = useCallback(async (text: string) => {
    try {
      await api.copyText(text);
      toast.success("Copied");
    } catch (e) {
      toast.error(String(e));
    }
  }, []);

  const handleInsert = useCallback(async (text: string) => {
    try {
      await api.pasteClipboard(text);
      toast.success("Inserted into the previous application");
    } catch (e) {
      toast.error(String(e));
    }
  }, []);

  const handleDelete = useCallback(async (id: number) => {
    const ok = await confirmDialog({
      title: "Delete dictation?",
      description: "This entry will be removed permanently.",
      confirmLabel: "Delete",
      destructive: true,
    });
    if (!ok) return;
    try {
      await api.deleteHistory(id);
      setSelectedIds((prev) => {
        const next = new Set(prev);
        next.delete(id);
        return next;
      });
      afterChange((entries) => entries.filter((entry) => entry.id !== id));
      toast.success("Deleted");
      await useStore.getState().refreshAll();
    } catch (e) {
      toast.error(String(e));
    }
  }, [afterChange]);

  const [editOriginal, setEditOriginal] = useState("");
  const beginEdit = useCallback((entry: api.HistoryEntry) => {
    setEditingId(entry.id);
    setEditText(entry.text);
    setEditOriginal(entry.text);
  }, []);

  const toggleExpand = useCallback((id: number) => setExpandedId((current) => (current === id ? null : id)), []);
  const cancelEdit = useCallback(() => setEditingId(null), []);

  const saveEdit = async () => {
    if (editingId === null || !editText.trim()) return;
    const id = editingId;
    const text = editText.trim();
    try {
      await api.updateHistory(id, text);
      setEditingId(null);
      afterChange((entries) => entries.map((entry) => (entry.id === id ? { ...entry, text } : entry)));
      const words = wordsWorthLearning(editOriginal, text, dictionary.map((entry) => entry.word));
      setLearn(words.length > 0 ? words : null);
      await useStore.getState().refreshAll();
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleClearAll = async () => {
    const ok = await confirmDialog({
      title: "Clear all history?",
      description: "Every dictation entry will be removed permanently. This cannot be undone.",
      confirmLabel: "Clear all",
      destructive: true,
    });
    if (!ok) return;
    try {
      await api.clearHistory();
      setSelectedIds(new Set());
      setOlder([]);
      setOlderDone(false);
      setSearchRun((n) => n + 1);
      toast.success("History cleared");
      await useStore.getState().refreshAll();
    } catch (e) {
      toast.error(String(e));
    }
  };

  return (
    <div className="flex flex-col gap-4">
      {speedStats && (
        <div className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-border bg-card px-4 py-3">
          <div className="flex items-center gap-2.5">
            
            <div className="flex flex-col">
              <div className="flex items-center gap-2">
                <span className="text-sm font-medium text-foreground">
                  Average speed <span className="font-semibold text-brand tabular-nums">{speedStats.avgWpm} WPM</span>
                </span>
                <span className="rounded-sm bg-positive-soft px-1.5 py-0.5 text-xs font-medium text-positive">
                  {speedStats.multiplier}× typing speed
                </span>
              </div>
              <p className="text-xs text-muted-foreground">
                You dictated {speedStats.totalWords.toLocaleString()} words across {history.length >= 500 ? "your newest 500" : history.length} session{history.length === 1 ? "" : "s"} at ~{speedStats.multiplier}× average typing speed.
              </p>
            </div>
          </div>
        </div>
      )}

      <div className="flex items-center gap-2">
        <Input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search all history…"
          aria-label="Search all history"
        />
        <Button variant="outline" onClick={() => handleExport("json")}>
          Export JSON
        </Button>
        <Button variant="outline" onClick={() => handleExport("csv")}>
          Export CSV
        </Button>
        <Button variant="outline" onClick={handleClearAll}>
          Clear all
        </Button>
      </div>

      {exported && (
        <div className="flex flex-wrap items-center gap-2 rounded-md bg-positive-soft px-3 py-2 text-[13px] text-positive ">
          <span className="truncate">Exported to {exported}</span>
          <Button
            size="sm"
            variant="outline"
            className="ml-auto"
            onClick={handleReveal}
          >
            Show in folder
          </Button>
        </div>
      )}

      {learn && <LearnWords key={learn.join("\n")} words={learn} onDone={() => setLearn(null)} />}

      {selectedIds.size > 0 && (
        <div className="flex items-center justify-between border border-border bg-card p-2">
          <span className="text-xs font-semibold ">
            {selectedIds.size} selected
          </span>
          <div className="flex items-center gap-2">
            <Button size="sm" variant="ghost" onClick={() => setSelectedIds(new Set())}>
              Deselect all
            </Button>
            <Button size="sm" variant="destructive" onClick={handleBulkDelete}>
              Delete selected ({selectedIds.size})
            </Button>
          </div>
        </div>
      )}

      {!hydrated && loaded.length === 0 ? (
        <Card>
          <CardContent className="flex flex-col gap-2 p-4">
            {[0, 1, 2, 3].map((i) => (
              <Skeleton key={i} className="h-10 w-full" />
            ))}
          </CardContent>
        </Card>
      ) : filtered.length === 0 ? (
        <div className="rounded-lg border border-dashed border-input p-6 text-center">
          <p className="text-sm font-semibold">
            {loaded.length === 0
              ? "No dictations yet"
              : "No entries match your search"}
          </p>
          <p className="mt-1 text-sm text-muted-foreground">
            {loaded.length === 0
              ? `Press ${hotkey} in any app and speak.`
              : "Try a different query."}
          </p>
        </div>
      ) : (
        <>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead className="w-10">
                  <input
                    type="checkbox"
                    className="size-4 cursor-pointer accent-primary"
                    checked={allVisibleSelected}
                    ref={(input) => { if (input) input.indeterminate = someVisibleSelected; }}
                    onChange={toggleSelectAll}
                    aria-label="Select all dictations"
                  />
                </TableHead>
                <TableHead>Text</TableHead>
                <TableHead className="w-24 text-right">Speed</TableHead>
                <TableHead className="w-36 text-right"><span className="sr-only">Actions</span></TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {visible.map((entry) => {
                const editing = editingId === entry.id;
                return (
                  <HistoryRow
                    key={entry.id}
                    entry={entry}
                    selected={selectedIds.has(entry.id)}
                    expanded={expandedId === entry.id}
                    editing={editing}
                    editText={editing ? editText : undefined}
                    onEditText={editing ? setEditText : undefined}
                    onSave={editing ? saveEdit : undefined}
                    onCancel={editing ? cancelEdit : undefined}
                    onToggleSelect={toggleSelectOne}
                    onToggleExpand={toggleExpand}
                    onBeginEdit={beginEdit}
                    onInsert={handleInsert}
                    onCopy={handleCopy}
                    onDelete={handleDelete}
                  />
                );
              })}
            </TableBody>
          </Table>
          {moreToDraw && <div ref={listEnd} aria-hidden className="h-px" />}
          {trimmedQuery && results?.query === trimmedQuery && results.entries.length >= PAGE && (
            <p className="text-center text-xs text-muted-foreground">
              Showing the newest {PAGE} matches. Add words to narrow the search.
            </p>
          )}
          {canShowOlder && !moreToDraw && (
            <Button variant="outline" className="self-center" disabled={loadingOlder} onClick={showOlder}>
              {loadingOlder ? "Loading…" : "Show older dictations"}
            </Button>
          )}
        </>
      )}
    </div>
  );
}
