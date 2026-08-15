import { useMemo, useState } from "react";
import { useStore } from "@/lib/store";
import * as api from "@/lib/api";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";

function formatDate(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  return date.toLocaleString();
}

export function HistoryTab() {
  const history = useStore((s) => s.history);
  const [query, setQuery] = useState("");

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return history;
    return history.filter((entry) => entry.text.toLowerCase().includes(q));
  }, [history, query]);

  const handleCopy = async (text: string) => {
    try {
      await api.pasteClipboard(text);
    } catch {}
  };

  const handleDelete = async (id: number) => {
    try {
      await api.deleteHistory(id);
      await useStore.getState().refreshAll();
    } catch {}
  };

  const handleClearAll = async () => {
    if (!window.confirm("Clear all history? This cannot be undone.")) return;
    try {
      await api.clearHistory();
      await useStore.getState().refreshAll();
    } catch {}
  };

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-center gap-2">
        <Input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search history…"
        />
        <Button variant="destructive" onClick={handleClearAll}>
          Clear all
        </Button>
      </div>
      {filtered.length === 0 ? (
        <p className="text-sm text-[#64748B]">
          {history.length === 0
            ? "No dictations yet. Press the hotkey and speak."
            : "No entries match your search."}
        </p>
      ) : (
        <div className="rounded-xl border border-border">
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
                  <TableCell className="max-w-0 truncate whitespace-nowrap">
                    <span className="block max-w-[280px] truncate text-[#F8FAFC]">
                      {entry.text}
                    </span>
                  </TableCell>
                  <TableCell className="text-[#64748B]">
                    {formatDate(entry.created_at)}
                  </TableCell>
                  <TableCell>
                    <Badge variant="secondary">{entry.source}</Badge>
                  </TableCell>
                  <TableCell className="text-right">
                    <div className="flex items-center justify-end gap-1">
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