import { useState, type ChangeEvent } from "react";
import { useStore } from "@/lib/store";
import * as api from "@/lib/api";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";

export function DictionaryTab() {
  const dictionary = useStore((s) => s.dictionary);
  const [word, setWord] = useState("");
  const [bulkWords, setBulkWords] = useState("");
  const [message, setMessage] = useState<string | null>(null);

  const handleAdd = async () => {
    const trimmed = word.trim();
    if (!trimmed) return;
    try {
      await api.addDictionaryWord(trimmed);
      setWord("");
      setMessage(`Added “${trimmed}”`);
      await useStore.getState().refreshAll();
    } catch {}
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
      setMessage(`Added ${words.length} term${words.length === 1 ? "" : "s"}`);
      await useStore.getState().refreshAll();
    } catch {}
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
    setMessage("Dictionary exported");
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
      setMessage(`Imported ${words.length} term${words.length === 1 ? "" : "s"}`);
      await useStore.getState().refreshAll();
    } catch {}
    event.target.value = "";
  };

  const handleRemove = async (w: string) => {
    try {
      await api.removeDictionaryWord(w);
      await useStore.getState().refreshAll();
    } catch {}
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
          <label className="inline-flex cursor-pointer items-center border-2 border-black bg-white px-3 py-2 text-xs font-bold uppercase shadow-[3px_3px_0_0_#E8E8E8] hover:bg-black hover:text-white">
            Import dictionary
            <input type="file" accept=".txt,.csv" className="sr-only" onChange={handleImport} />
          </label>
        </div>
      </div>
      {message && (
        <div className="border-2 border-black bg-black px-2 py-1.5 text-xs font-bold text-white uppercase">
          ✓ {message}
        </div>
      )}
      {dictionary.length === 0 ? (
        <div className="border-2 border-dashed border-black p-6 text-center">
          <p className="text-sm font-bold uppercase tracking-wider">
            No custom words yet
          </p>
          <p className="mt-1 text-sm text-muted-foreground">
            Add words to improve recognition for names, jargon, and acronyms.
          </p>
        </div>
      ) : (
        <div className="flex flex-wrap gap-2">
          {dictionary.map((entry) => (
            <Badge key={entry.id} variant="outline" className="gap-1.5 py-1 pr-1.5 pl-2.5">
              {entry.word}
              <button
                onClick={() => handleRemove(entry.word)}
                className="flex size-4 items-center justify-center border border-black bg-white text-xs font-bold transition-colors hover:bg-black hover:text-white"
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
