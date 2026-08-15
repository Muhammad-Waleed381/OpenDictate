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

function formatDate(value: string): string {
  const num = Number(value);
  const date = new Date(Number.isFinite(num) && num < 1e12 ? num * 1000 : value);
  if (Number.isNaN(date.getTime())) return value;
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
        <Button variant="outline" onClick={handleClearAll}>
          Clear all
        </Button>
      </div>
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
                  <TableCell className="max-w-0 truncate whitespace-nowrap">
                    <span className="block max-w-[280px] truncate font-medium">
                      {entry.text}
                    </span>
                  </TableCell>
                  <TableCell className="text-muted-foreground tabular-nums">
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