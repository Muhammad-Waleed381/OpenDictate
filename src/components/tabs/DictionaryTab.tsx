import { useState, type ChangeEvent } from "react";
import { useStore } from "@/lib/store";
import * as api from "@/lib/api";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { toast } from "@/components/ui/toast";

export function DictionaryTab() {
  const dictionary = useStore((s) => s.dictionary);
  const [word, setWord] = useState("");
  const [bulkWords, setBulkWords] = useState("");
  const [searchQuery, setSearchQuery] = useState("");

  const filteredDictionary = dictionary.filter((entry) =>
    entry.word.toLowerCase().includes(searchQuery.trim().toLowerCase())
  );

  const handleAdd = async () => {
    const trimmed = word.trim();
    if (!trimmed) return;
    try {
      await api.addDictionaryWord(trimmed);
      setWord("");
      toast.success(`Added “${trimmed}”`);
      await useStore.getState().refreshAll();
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleBulkAdd = async () => {
    const words = bulkWords
      .split(/[\n,]/)
      .map((value) => value.trim())
      .filter(Boolean);
    if (words.length === 0) return;
    try {
      for (const value of words) await api.addDictionaryWord(value);
      setBulkWords("");
      toast.success(`Added ${words.length} term${words.length === 1 ? "" : "s"}`);
      await useStore.getState().refreshAll();
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleExport = async () => {
    const blob = new Blob([dictionary.map((entry) => entry.word).join("\n")], {
      type: "text/plain;charset=utf-8",
    });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = "opendictate-dictionary.txt";
    link.click();
    URL.revokeObjectURL(url);
    toast.success("Dictionary exported");
  };

  const handleImport = async (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    if (!file) return;
    const words = (await file.text())
      .split(/[\n,]/)
      .map((value) => value.trim())
      .filter(Boolean);
    try {
      for (const value of words) await api.addDictionaryWord(value);
      toast.success(`Imported ${words.length} term${words.length === 1 ? "" : "s"}`);
      await useStore.getState().refreshAll();
    } catch (e) {
      toast.error(String(e));
    }
    event.target.value = "";
  };

  const handleRemove = async (w: string) => {
    try {
      await api.removeDictionaryWord(w);
      toast.success(`Removed “${w}”`);
      await useStore.getState().refreshAll();
    } catch (e) {
      toast.error(String(e));
    }
  };

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-center gap-2">
        <Input
          value={word}
          onChange={(e) => setWord(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") handleAdd();
          }}
          placeholder="Add a word…"
        />
        <Button onClick={handleAdd} disabled={!word.trim()}>
          + Add
        </Button>
      </div>
      <div className="flex flex-col gap-2">
        <Textarea
          value={bulkWords}
          onChange={(event) => setBulkWords(event.target.value)}
          placeholder="Paste several terms, one per line or separated by commas…"
          rows={3}
        />
        <div className="flex flex-wrap gap-2">
          <Button variant="outline" onClick={handleBulkAdd} disabled={!bulkWords.trim()}>
            Add list
          </Button>
          <Button variant="outline" onClick={handleExport} disabled={dictionary.length === 0}>
            Export dictionary
          </Button>
          <label className="inline-flex cursor-pointer items-center border-2 border-border bg-card px-3 py-2 text-xs font-bold uppercase shadow-brutal hover:bg-primary hover:text-primary-foreground">
            Import dictionary
            <input type="file" accept=".txt,.csv" className="sr-only" onChange={handleImport} />
          </label>
        </div>
      </div>

      {dictionary.length > 0 && (
        <div className="flex items-center justify-between gap-2 border-t border-border pt-2">
          <Input
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder="Search dictionary terms…"
            className="max-w-xs"
          />
          <span className="text-xs font-bold uppercase text-muted-foreground tabular-nums">
            Showing {filteredDictionary.length} of {dictionary.length} words
          </span>
        </div>
      )}

      {dictionary.length === 0 ? (
        <div className="border-2 border-dashed border-border p-6 text-center">
          <p className="text-sm font-bold uppercase tracking-wider">
            No custom words yet
          </p>
          <p className="mt-1 text-sm text-muted-foreground">
            Add words to improve recognition for names, jargon, and acronyms.
          </p>
        </div>
      ) : filteredDictionary.length === 0 ? (
        <div className="border-2 border-dashed border-border p-6 text-center">
          <p className="text-sm font-bold uppercase tracking-wider">
            No words match “{searchQuery}”
          </p>
          <p className="mt-1 text-sm text-muted-foreground">
            Try a different search term or clear the filter.
          </p>
        </div>
      ) : (
        <div className="flex flex-wrap gap-2">
          {filteredDictionary.map((entry) => (
            <Badge key={entry.id} variant="outline" className="gap-1.5 py-1 pr-1.5 pl-2.5">
              {entry.word}
              <button
                onClick={() => handleRemove(entry.word)}
                className="flex size-4 items-center justify-center border border-border bg-card text-xs font-bold transition-colors hover:bg-destructive hover:text-destructive-foreground"
                aria-label={`Remove ${entry.word}`}
              >
                ×
              </button>
            </Badge>
          ))}
        </div>
      )}
    </div>
  );
}
