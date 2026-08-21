import { useState, type ChangeEvent } from "react";
import { useStore } from "@/lib/store";
import * as api from "@/lib/api";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
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

export function SnippetsTab() {
  const snippets = useStore((s) => s.snippets);
  const lastResult = useStore((s) => s.lastResult);
  const [trigger, setTrigger] = useState("");
  const [text, setText] = useState("");
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [exported, setExported] = useState<string | null>(null);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [editTrigger, setEditTrigger] = useState("");
  const [editText, setEditText] = useState("");

  const handleAdd = async () => {
    setError(null);
    if (!trigger.trim() || !text.trim()) return;
    try {
      await api.addSnippet(trigger, text);
      setTrigger("");
      setText("");
      setMessage(`Added “${trigger.trim()}”`);
      await useStore.getState().refreshAll();
    } catch (e) {
      setError(String(e));
    }
  };

  const handleQuickCapture = () => {
    if (!lastResult?.text) return;
    setText(lastResult.text);
    setTrigger("");
    setMessage("Filled from last dictation — give it a trigger name");
  };

  const beginEdit = (entry: api.SnippetEntry) => {
    setEditingId(entry.id);
    setEditTrigger(entry.trigger);
    setEditText(entry.text);
  };

  const saveEdit = async (id: number) => {
    setError(null);
    if (!editTrigger.trim() || !editText.trim()) return;
    try {
      await api.updateSnippet(id, editTrigger, editText);
      setEditingId(null);
      setMessage("Snippet updated");
      await useStore.getState().refreshAll();
    } catch (e) {
      setError(String(e));
    }
  };

  const handleDelete = async (id: number, triggerName: string) => {
    const ok = await confirmDialog({
      title: "Delete snippet?",
      description: `“${triggerName}” will be removed permanently.`,
      confirmLabel: "Delete",
      destructive: true,
    });
    if (!ok) return;
    try {
      await api.removeSnippet(id);
      toast.success(`Deleted “${triggerName}”`);
      await useStore.getState().refreshAll();
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleExport = async () => {
    setError(null);
    try {
      const path = await api.exportSnippets();
      setExported(path);
    } catch (e) {
      setExported(null);
      setError(String(e));
    }
  };

  const handleReveal = async () => {
    if (!exported) return;
    try {
      await revealItemInDir(exported);
    } catch {}
  };

  const handleImport = async (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    if (!file) return;
    setError(null);
    try {
      const imported = await api.importSnippets(await file.text());
      setMessage(`Imported ${imported} snippet${imported === 1 ? "" : "s"}`);
      await useStore.getState().refreshAll();
    } catch (e) {
      setError(String(e));
    }
    event.target.value = "";
  };

  return (
    <div className="flex flex-col gap-4">
      <Card>
        <CardContent className="flex flex-col gap-2 p-3">
        <div className="flex flex-wrap items-center gap-2">
          <Input
            value={trigger}
            onChange={(e) => setTrigger(e.target.value)}
            placeholder="One-word trigger — e.g. “signature”"
            className="flex-1 min-w-40"
            aria-label="Snippet trigger name"
          />
          <Button onClick={handleAdd} disabled={!trigger.trim() || !text.trim()}>
            + Add
          </Button>
          <Button
            variant="outline"
            onClick={handleQuickCapture}
            disabled={!lastResult?.text}
            title={lastResult?.text ? `From: “${lastResult.text}”` : "No recent dictation"}
          >
            From last dictation
          </Button>
        </div>
        <Textarea
          value={text}
          onChange={(event) => setText(event.target.value)}
          placeholder="Template text that gets inserted when you say: “insert snippet <trigger>”…"
          rows={3}
        />
        </CardContent>
      </Card>

      <div className="flex flex-wrap gap-2">
        <Button variant="outline" onClick={handleExport} disabled={snippets.length === 0}>
          Export snippets
        </Button>
        <label className="inline-flex cursor-pointer items-center border-2 border-black bg-white px-3 py-2 text-xs font-bold uppercase shadow-[3px_3px_0_0_#E8E8E8] hover:bg-black hover:text-white">
          Import snippets
          <input type="file" accept=".json" className="sr-only" onChange={handleImport} />
        </label>
        {exported && (
          <Button variant="outline" onClick={handleReveal}>
            Reveal export
          </Button>
        )}
      </div>

      {message && (
        <div className="border-2 border-black bg-black px-2 py-1.5 text-xs font-bold text-white uppercase">
          ✓ {message}
        </div>
      )}
      {error && (
        <div className="border-2 border-black bg-black px-2 py-1.5 text-xs font-bold text-white uppercase">
          ✕ {error}
        </div>
      )}

      {snippets.length === 0 ? (
        <div className="border-2 border-dashed border-black p-6 text-center">
          <p className="text-sm font-bold uppercase tracking-wider">No snippets yet</p>
          <p className="mt-1 text-sm text-muted-foreground">
            Add a template above, then dictate “insert snippet &lt;trigger&gt;” to expand it.
            Anything said after the trigger is dictated normally.
          </p>
        </div>
      ) : (
        <div className="border-2 border-black">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Trigger</TableHead>
                <TableHead>Text</TableHead>
                <TableHead className="w-32">Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {snippets.map((entry) =>
                editingId === entry.id ? (
                  <TableRow key={entry.id}>
                    <TableCell className="align-top">
                      <Input
                        value={editTrigger}
                        onChange={(e) => setEditTrigger(e.target.value)}
                        aria-label="Edit trigger name"
                      />
                    </TableCell>
                    <TableCell className="align-top">
                      <Textarea
                        value={editText}
                        onChange={(e) => setEditText(e.target.value)}
                        rows={3}
                        aria-label="Edit snippet text"
                      />
                    </TableCell>
                    <TableCell className="align-top">
                      <div className="flex flex-col gap-1.5">
                        <Button size="sm" onClick={() => saveEdit(entry.id)}>
                          Save
                        </Button>
                        <Button
                          size="sm"
                          variant="outline"
                          onClick={() => setEditingId(null)}
                        >
                          Cancel
                        </Button>
                      </div>
                    </TableCell>
                  </TableRow>
                ) : (
                  <TableRow key={entry.id}>
                    <TableCell className="font-bold whitespace-nowrap">
                      {entry.trigger}
                    </TableCell>
                    <TableCell className="max-w-md">
                      <span className="block truncate">{entry.text}</span>
                    </TableCell>
                    <TableCell>
                      <div className="flex flex-col gap-1.5">
                        <Button size="sm" variant="outline" onClick={() => beginEdit(entry)}>
                          Edit
                        </Button>
                        <Button
                          size="sm"
                          variant="outline"
                          onClick={() => handleDelete(entry.id, entry.trigger)}
                        >
                          Delete
                        </Button>
                      </div>
                    </TableCell>
                  </TableRow>
                )
              )}
            </TableBody>
          </Table>
        </div>
      )}
    </div>
  );
}