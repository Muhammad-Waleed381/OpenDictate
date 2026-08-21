import { useMemo, useState } from "react";
import { useStore } from "@/lib/store";
import * as api from "@/lib/api";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { toast } from "@/components/ui/toast";
import { confirmDialog } from "@/components/ui/confirm-dialog";
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

export function HistoryTab() {
  const history = useStore((s) => s.history);
  const [query, setQuery] = useState("");
  const [exported, setExported] = useState<string | null>(null);
  const [exportError, setExportError] = useState<string | null>(null);
  const [expandedId, setExpandedId] = useState<number | null>(null);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [editText, setEditText] = useState("");

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return history;
    return history.filter((entry) => entry.text.toLowerCase().includes(q));
  }, [history, query]);

  const handleExport = async (kind: api.ExportKind) => {
    setExportError(null);
    try {
      const path = await api.exportHistory(kind);
      setExported(path);
    } catch (e) {
      setExported(null);
      setExportError(String(e));
    }
  };

  const handleReveal = async () => {
    if (!exported) return;
    try {
      await revealItemInDir(exported);
    } catch {}
  };

  const handleCopy = async (text: string) => {
    try {
      await api.copyText(text);
    } catch {}
  };

  const handleInsert = async (text: string) => {
    try {
      await api.pasteClipboard(text);
    } catch {}
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
      setExportError(String(e));
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
        <div className="flex flex-wrap items-center gap-2 border-2 border-black bg-black px-2 py-1.5 text-xs font-bold text-white uppercase">
          <span className="truncate">✓ Exported — {exported}</span>
          <Button
            size="sm"
            variant="outline"
            className="ml-auto border-white text-white shadow-none"
            onClick={handleReveal}
          >
            Show in folder
          </Button>
        </div>
      )}
      {exportError && (
        <div className="border-2 border-black bg-black px-2 py-1.5 text-xs font-bold text-white uppercase">
          ✕ {exportError}
        </div>
      )}
      {filtered.length === 0 ? (
        <div className="border-2 border-dashed border-black p-6 text-center">
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
        <div className="border-2 border-black shadow-[6px_6px_0_0_#E8E8E8]">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead className="w-[45%]">Text</TableHead>
                <TableHead>Date</TableHead>
                <TableHead>Source</TableHead>
                <TableHead className="text-right">Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {filtered.map((entry) => (
                <TableRow key={entry.id}>
                  <TableCell className="max-w-0">
                    {editingId === entry.id ? (
                      <div className="flex min-w-[220px] flex-col gap-2">
                        <textarea
                          value={editText}
                          onChange={(event) => setEditText(event.target.value)}
                          rows={3}
                          className="w-full border-2 border-black bg-white p-2 text-sm"
                        />
                        <div className="flex gap-1">
                          <Button size="sm" onClick={saveEdit}>Save</Button>
                          <Button size="sm" variant="ghost" onClick={() => setEditingId(null)}>Cancel</Button>
                        </div>
                      </div>
                    ) : (
                      <button
                        className={`block max-w-[360px] text-left font-medium ${expandedId === entry.id ? "whitespace-pre-wrap" : "truncate"}`}
                        onClick={() => setExpandedId(expandedId === entry.id ? null : entry.id)}
                        title="Click to expand"
                      >
                        {entry.text}
                      </button>
                    )}
                  </TableCell>
                  <TableCell className="text-muted-foreground tabular-nums">
                    {formatDate(entry.created_at)}
                  </TableCell>
                  <TableCell>
                    <Badge variant="secondary">{entry.source}</Badge>
                  </TableCell>
                  <TableCell className="text-right">
                    <div className="flex items-center justify-end gap-1">
                       <Button size="sm" variant="ghost" onClick={() => beginEdit(entry)}>
                         Edit
                       </Button>
                       <Button size="sm" variant="ghost" onClick={() => handleInsert(entry.text)}>
                        Re-insert
                      </Button>
                      <Button size="sm" variant="ghost" onClick={() => handleCopy(entry.text)}>
                        Copy
                      </Button>
                      <Button size="sm" variant="ghost" onClick={() => handleDelete(entry.id)}>
                        Delete
                      </Button>
                    </div>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
      )}
    </div>
  );
}
