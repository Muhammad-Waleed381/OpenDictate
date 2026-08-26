import { useMemo, useState } from "react";
import { useStore } from "@/lib/store";
import * as api from "@/lib/api";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { toast } from "@/components/ui/toast";
import { confirmDialog } from "@/components/ui/confirm-dialog";
import { ClipboardPaste, Copy, Pencil, Trash2 } from "lucide-react";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";

function formatDate(value: string): string {
  const num = Number(value);
  const date = new Date(Number.isFinite(num) && num < 1e12 ? num * 1000 : value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString();
}

function formatDuration(ms: number): string {
  if (!ms || ms <= 0) return "< 1s";
  const sec = ms / 1000;
  if (sec < 60) return `${sec.toFixed(1)}s`;
  const mins = Math.floor(sec / 60);
  const remSec = Math.round(sec % 60);
  return `${mins}m ${remSec}s`;
}

export function HistoryTab() {
  const history = useStore((s) => s.history);
  const hydrated = useStore((s) => s.hydrated);
  const [query, setQuery] = useState("");
  const [exported, setExported] = useState<string | null>(null);
  const [expandedId, setExpandedId] = useState<number | null>(null);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [editText, setEditText] = useState("");
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set());

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return history;
    return history.filter((entry) => entry.text.toLowerCase().includes(q));
  }, [history, query]);

  const toggleSelectAll = () => {
    if (selectedIds.size === filtered.length) {
      setSelectedIds(new Set());
    } else {
      setSelectedIds(new Set(filtered.map((e) => e.id)));
    }
  };

  const toggleSelectOne = (id: number) => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const handleBulkDelete = async () => {
    const count = selectedIds.size;
    if (count === 0) return;
    const ok = await confirmDialog({
      title: `Delete ${count} dictations?`,
      description: `The selected ${count} entries will be permanently removed.`,
      confirmLabel: `Delete (${count})`,
      destructive: true,
    });
    if (!ok) return;
    try {
      for (const id of selectedIds) {
        await api.deleteHistory(id);
      }
      setSelectedIds(new Set());
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
      toast.info(`Exported — ${path}`);
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

  const handleCopy = async (text: string) => {
    try {
      await api.copyText(text);
      toast.success("Copied");
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleInsert = async (text: string) => {
    try {
      await api.pasteClipboard(text);
      toast.success("Sent to clipboard");
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleDelete = async (id: number) => {
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
      toast.success("Deleted");
      await useStore.getState().refreshAll();
    } catch (e) {
      toast.error(String(e));
    }
  };

  const beginEdit = (entry: api.HistoryEntry) => {
    setEditingId(entry.id);
    setEditText(entry.text);
  };

  const saveEdit = async () => {
    if (editingId === null || !editText.trim()) return;
    try {
      await api.updateHistory(editingId, editText);
      setEditingId(null);
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
      toast.success("History cleared");
      await useStore.getState().refreshAll();
    } catch (e) {
      toast.error(String(e));
    }
  };

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-center gap-2">
        <Input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search history…"
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
        <div className="flex flex-wrap items-center gap-2 border-2 border-primary bg-primary px-2 py-1.5 text-xs font-bold text-primary-foreground uppercase">
          <span className="truncate">✓ Exported — {exported}</span>
          <Button
            size="sm"
            variant="outline"
            className="ml-auto border-primary-foreground text-primary-foreground shadow-none"
            onClick={handleReveal}
          >
            Show in folder
          </Button>
        </div>
      )}

      {selectedIds.size > 0 && (
        <div className="flex items-center justify-between border-2 border-border bg-card p-2 shadow-brutal">
          <span className="text-xs font-bold uppercase">
            {selectedIds.size} of {filtered.length} selected
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

      {!hydrated ? (
        <Card>
          <CardContent className="flex flex-col gap-2 p-4">
            {[0, 1, 2, 3].map((i) => (
              <Skeleton key={i} className="h-10 w-full" />
            ))}
          </CardContent>
        </Card>
      ) : filtered.length === 0 ? (
        <div className="border-2 border-dashed border-border p-6 text-center">
          <p className="text-sm font-bold uppercase tracking-wider">
            {history.length === 0
              ? "No dictations yet"
              : "No entries match your search"}
          </p>
          <p className="mt-1 text-sm text-muted-foreground">
            {history.length === 0
              ? "Press the hotkey and speak."
              : "Try a different query."}
          </p>
        </div>
      ) : (
        <Card>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead className="w-10">
                  <input
                    type="checkbox"
                    className="size-4 cursor-pointer accent-primary"
                    checked={filtered.length > 0 && selectedIds.size === filtered.length}
                    onChange={toggleSelectAll}
                    aria-label="Select all dictations"
                  />
                </TableHead>
                <TableHead className="w-[40%]">Text</TableHead>
                <TableHead>Duration</TableHead>
                <TableHead>Date</TableHead>
                <TableHead>Source</TableHead>
                <TableHead className="text-right">Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {filtered.map((entry) => (
                <TableRow key={entry.id} className={selectedIds.has(entry.id) ? "bg-muted/40" : ""}>
                  <TableCell>
                    <input
                      type="checkbox"
                      className="size-4 cursor-pointer accent-primary"
                      checked={selectedIds.has(entry.id)}
                      onChange={() => toggleSelectOne(entry.id)}
                      aria-label={`Select dictation ${entry.id}`}
                    />
                  </TableCell>
                  <TableCell className="max-w-0">
                    {editingId === entry.id ? (
                      <div className="flex min-w-[220px] flex-col gap-2">
                        <Textarea
                          value={editText}
                          onChange={(event) => setEditText(event.target.value)}
                          rows={3}
                          aria-label="Edit dictation text"
                        />
                        <div className="flex gap-1">
                          <Button size="sm" onClick={saveEdit}>Save</Button>
                          <Button size="sm" variant="ghost" onClick={() => setEditingId(null)}>Cancel</Button>
                        </div>
                      </div>
                    ) : (
                      <button
                        className={`block max-w-[340px] text-left font-medium ${expandedId === entry.id ? "whitespace-pre-wrap" : "truncate"}`}
                        onClick={() => setExpandedId(expandedId === entry.id ? null : entry.id)}
                        title="Click to expand"
                      >
                        {entry.text}
                      </button>
                    )}
                  </TableCell>
                  <TableCell className="text-xs font-bold text-muted-foreground tabular-nums whitespace-nowrap">
                    {formatDuration(entry.duration_ms)}
                  </TableCell>
                  <TableCell className="text-xs text-muted-foreground tabular-nums whitespace-nowrap">
                    {formatDate(entry.created_at)}
                  </TableCell>
                  <TableCell>
                    <Badge variant="secondary">{entry.source}</Badge>
                  </TableCell>
                  <TableCell className="text-right">
                    <div className="flex items-center justify-end gap-1">
                      <Button size="icon-sm" variant="ghost" title="Edit" onClick={() => beginEdit(entry)}>
                        <Pencil />
                      </Button>
                      <Button size="icon-sm" variant="ghost" title="Re-insert" onClick={() => handleInsert(entry.text)}>
                        <ClipboardPaste />
                      </Button>
                      <Button size="icon-sm" variant="ghost" title="Copy" onClick={() => handleCopy(entry.text)}>
                        <Copy />
                      </Button>
                      <Button size="icon-sm" variant="destructive" title="Delete" onClick={() => handleDelete(entry.id)}>
                        <Trash2 />
                      </Button>
                    </div>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </Card>
      )}
    </div>
  );
}
