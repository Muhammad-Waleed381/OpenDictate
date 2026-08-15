import { useState } from "react";
import { useStore } from "@/lib/store";
import * as api from "@/lib/api";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";

export function DictionaryTab() {
  const dictionary = useStore((s) => s.dictionary);
  const [word, setWord] = useState("");

  const handleAdd = async () => {
    const trimmed = word.trim().toLowerCase();
    if (!trimmed) return;
    try {
      await api.addDictionaryWord(trimmed);
      setWord("");
      await useStore.getState().refreshAll();
    } catch {}
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