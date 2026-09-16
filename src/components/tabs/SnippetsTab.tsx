import { useState, useRef, type ChangeEvent } from "react";
import { useStore } from "@/lib/store";
import * as api from "@/lib/api";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { toast } from "@/components/ui/toast";
import { confirmDialog } from "@/components/ui/confirm-dialog";
import { Copy, CornerDownLeft, Pencil, Trash2 } from "lucide-react";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";

const SNIPPET_PLACEHOLDERS = ["{date}", "{time}", "{datetime}", "{clipboard}"] as const;

export function SnippetsTab() {
  const snippets = useStore((s) => s.snippets);
  const lastResult = useStore((s) => s.lastResult);
  const [trigger, setTrigger] = useState("");
  const [text, setText] = useState("");
  const [lastExportPath, setLastExportPath] = useState<string | null>(null);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [editTrigger, setEditTrigger] = useState("");
  const [editText, setEditText] = useState("");
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);
  const editTextareaRef = useRef<HTMLTextAreaElement | null>(null);

  const handleInsertPlaceholder = (token: string, isEdit = false) => {
    const textarea = isEdit ? editTextareaRef.current : textareaRef.current;
    const current = isEdit ? editText : text;

    if (textarea && document.activeElement === textarea) {
      const start = textarea.selectionStart ?? current.length;
      const end = textarea.selectionEnd ?? current.length;
      const before = current.slice(0, start);
      const after = current.slice(end);
      const next = `${before}${token}${after}`;
      if (isEdit) {
        setEditText(next);
      } else {
        setText(next);
      }
      setTimeout(() => {
        textarea.focus();
        const nextPos = start + token.length;
        textarea.setSelectionRange(nextPos, nextPos);
      }, 0);
    } else {
      const next = current
        ? (current.endsWith(" ") ? `${current}${token}` : `${current} ${token}`)
        : token;
      if (isEdit) {
        setEditText(next);
      } else {
        setText(next);
      }
      if (textarea) {
        setTimeout(() => {
          textarea.focus();
        }, 0);
      }
    }
  };

  const handleAdd = async () => {
    if (!trigger.trim() || !text.trim()) return;
    try {
      await api.addSnippet(trigger, text);
      setTrigger("");
      setText("");
      toast.success(`Added “${trigger.trim()}”`);
      await useStore.getState().refreshAll();
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleQuickCapture = () => {
    if (!lastResult?.text) return;
    setText(lastResult.text);
    setTrigger("");
    toast.info("Filled from last dictation — give it a trigger name");
  };

  const beginEdit = (entry: api.SnippetEntry) => {
    setEditingId(entry.id);
    setEditTrigger(entry.trigger);
    setEditText(entry.text);
  };

  const saveEdit = async (id: number) => {
    if (!editTrigger.trim() || !editText.trim()) return;
    try {
      await api.updateSnippet(id, editTrigger, editText);
      setEditingId(null);
      toast.success("Snippet updated");
      await useStore.getState().refreshAll();
    } catch (e) {
      toast.error(String(e));
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

  const revealExport = (path: string) => {
    setLastExportPath(path);
  };

  const handleExport = async () => {
    try {
      const path = await api.exportSnippets();
      revealExport(path);
      toast.info(`Exported to ${path}`);
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleReveal = async () => {
    if (!lastExportPath) return;
    try {
      await revealItemInDir(lastExportPath);
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleCopySnippet = async (text: string) => {
    try {
      await api.copyText(text);
      toast.success("Copied");
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleImport = async (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    if (!file) return;
    try {
      const imported = await api.importSnippets(await file.text());
      toast.success(`Imported ${imported} snippet${imported === 1 ? "" : "s"}`);
      await useStore.getState().refreshAll();
    } catch (e) {
      toast.error(String(e));
    }
    event.target.value = "";
  };

  const triggerHasWhitespace = /\s/.test(trigger.trim());
  const triggerExists = snippets.some(
    (s) => s.trigger.toLowerCase() === trigger.trim().toLowerCase()
  );
  const isValidTrigger = trigger.trim().length > 0 && !triggerHasWhitespace && !triggerExists;

  const handleTestInsert = async (snippetText: string) => {
    try {
      await api.pasteClipboard(snippetText);
      toast.success("Inserted snippet into focused app");
    } catch (e) {
      toast.error(`Insertion failed: ${String(e)}`);
    }
  };

  return (
    <div className="flex flex-col gap-4">
      <Card>
        <CardContent className="flex flex-col gap-2 p-3">
        <div className="flex flex-wrap items-center gap-2">
          <div className="flex flex-1 flex-col gap-1 min-w-40">
            <Input
              value={trigger}
              onChange={(e) => setTrigger(e.target.value)}
              placeholder="One-word trigger — e.g. “signature”"
              aria-label="Snippet trigger name"
              className={triggerHasWhitespace || triggerExists ? "border-destructive focus-visible:ring-destructive" : ""}
            />
            {triggerHasWhitespace && (
              <span className="text-[11px] font-bold text-destructive">
                ✕ Triggers must be a single word without spaces.
              </span>
            )}
            {triggerExists && (
              <span className="text-[11px] font-bold text-destructive">
                ✕ A snippet with trigger “{trigger.trim()}” already exists.
              </span>
            )}
          </div>
          <Button onClick={handleAdd} disabled={!isValidTrigger || !text.trim()}>
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
        <div className="flex flex-col gap-1.5">
          <div className="flex flex-wrap items-center gap-1.5">
            <span className="text-xs font-bold uppercase tracking-wider text-muted-foreground">
              Placeholders:
            </span>
            {SNIPPET_PLACEHOLDERS.map((token) => (
              <button
                key={token}
                type="button"
                onMouseDown={(e) => e.preventDefault()}
                onClick={() => handleInsertPlaceholder(token, false)}
                className="inline-flex h-6 items-center border border-border bg-secondary/70 px-2 font-mono text-xs font-semibold text-secondary-foreground transition-all hover:bg-primary hover:text-primary-foreground active:translate-x-[1px] active:translate-y-[1px] cursor-pointer"
                title={`Insert ${token}`}
              >
                {token}
              </button>
            ))}
          </div>
          <Textarea
            ref={textareaRef}
            value={text}
            onChange={(event) => setText(event.target.value)}
            placeholder="Template text that gets inserted when you say: “insert snippet <trigger>”…"
            rows={3}
          />
          <p className="text-[11px] text-muted-foreground">
            Placeholders like &#123;date&#125;, &#123;time&#125;, &#123;datetime&#125;, and &#123;clipboard&#125; are dynamically replaced when inserted.
          </p>
        </div>
        </CardContent>
      </Card>

      <div className="flex flex-wrap gap-2">
        <Button variant="outline" onClick={handleExport} disabled={snippets.length === 0}>
          Export snippets
        </Button>
        <label className="inline-flex cursor-pointer items-center border-2 border-border bg-card px-3 py-2 text-xs font-bold uppercase shadow-brutal hover:bg-primary hover:text-primary-foreground">
          Import snippets
          <input type="file" accept=".json" className="sr-only" onChange={handleImport} />
        </label>
        {lastExportPath && (
          <Button variant="outline" onClick={handleReveal}>
            Reveal export
          </Button>
        )}
      </div>

      {snippets.length === 0 ? (
        <div className="border-2 border-dashed border-border p-6 text-center">
          <p className="text-sm font-bold uppercase tracking-wider">No snippets yet</p>
          <p className="mt-1 text-sm text-muted-foreground">
            Add a template above, then dictate “insert snippet &lt;trigger&gt;” to expand it.
            Anything said after the trigger is dictated normally.
          </p>
        </div>
      ) : (
        <div className="border-2 border-border">
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
                      <div className="flex flex-col gap-1.5">
                        <div className="flex flex-wrap items-center gap-1">
                          <span className="text-[11px] font-bold uppercase tracking-wider text-muted-foreground">
                            Placeholders:
                          </span>
                          {SNIPPET_PLACEHOLDERS.map((token) => (
                            <button
                              key={token}
                              type="button"
                              onMouseDown={(e) => e.preventDefault()}
                              onClick={() => handleInsertPlaceholder(token, true)}
                              className="inline-flex h-5 items-center border border-border bg-secondary/70 px-1.5 font-mono text-[11px] font-semibold text-secondary-foreground transition-all hover:bg-primary hover:text-primary-foreground active:translate-x-[1px] active:translate-y-[1px] cursor-pointer"
                              title={`Insert ${token}`}
                            >
                              {token}
                            </button>
                          ))}
                        </div>
                        <Textarea
                          ref={editTextareaRef}
                          value={editText}
                          onChange={(e) => setEditText(e.target.value)}
                          rows={3}
                          aria-label="Edit snippet text"
                        />
                        <p className="text-[10px] text-muted-foreground">
                          Placeholders like &#123;date&#125;, &#123;time&#125;, &#123;datetime&#125;, and &#123;clipboard&#125; are dynamically replaced when inserted.
                        </p>
                      </div>
                    </TableCell>
                    <TableCell className="align-top">
                      <div className="flex gap-1">
                        <Button size="sm" onClick={() => saveEdit(entry.id)}>
                          Save
                        </Button>
                        <Button
                          size="sm"
                          variant="ghost"
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
                      <div className="flex items-center justify-end gap-1">
                        <Button
                          size="icon-sm"
                          variant="ghost"
                          title="Insert into active app"
                          onClick={() => handleTestInsert(entry.text)}
                        >
                          <CornerDownLeft />
                        </Button>
                        <Button size="icon-sm" variant="ghost" title="Edit" onClick={() => beginEdit(entry)}>
                          <Pencil />
                        </Button>
                        <Button size="icon-sm" variant="ghost" title="Copy text" onClick={() => handleCopySnippet(entry.text)}>
                          <Copy />
                        </Button>
                        <Button size="icon-sm" variant="destructive" title="Delete" onClick={() => handleDelete(entry.id, entry.trigger)}>
                          <Trash2 />
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