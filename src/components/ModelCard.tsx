import { useEffect, useRef, useState } from "react";
import { useStore } from "@/lib/store";
import * as api from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Progress } from "@/components/ui/progress";
import { toast } from "@/components/ui/toast";
import { confirmDialog } from "@/components/ui/confirm-dialog";
import { Loader2 } from "lucide-react";

function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "—";
  if (bytes < 1024) return `${Math.round(bytes)} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function modelSize(model: { installed: boolean; disk_bytes: number; size_bytes: number }): number | null {
  if (model.installed && Number.isFinite(model.disk_bytes) && model.disk_bytes > 0) {
    return model.disk_bytes;
  }
  if (Number.isFinite(model.size_bytes) && model.size_bytes > 0) {
    return model.size_bytes;
  }
  return null;
}

function progressPercent(received: number, total: number): number {
  if (!Number.isFinite(received) || !Number.isFinite(total) || total <= 0) return 0;
  return Math.min(100, Math.round((received / total) * 100));
}

const SECTION_HINTS: Record<"streaming" | "non-streaming", string> = {
  streaming:
    "Live captions appear while you speak. Best for fast back-and-forth dictation.",
  "non-streaming":
    "Transcribes after you stop speaking. Best for longer recordings and accuracy."
};

const SECTION_LABELS: Record<"streaming" | "non-streaming", string> = {
  streaming: "Live captions",
  "non-streaming": "Highest accuracy",
};

export function ModelCard() {
  const catalog = useStore((s) => s.catalog);
  const settings = useStore((s) => s.settings);
  const modelProgress = useStore((s) => s.modelProgress);
  const modelsStatus = useStore((s) => s.models);
  const streamingTooSlow =
    (modelsStatus?.streaming_rtf_x100 ?? 0) > 150 && (modelsStatus?.streaming_rtf_x100 ?? 0) !== 0;
  const [startingDownloads, setStartingDownloads] = useState<Set<string>>(new Set());
  const [cancellingDownloads, setCancellingDownloads] = useState<Set<string>>(new Set());
  const [downloadAllStatus, setDownloadAllStatus] = useState<{
    current: number;
    total: number;
    name: string;
  } | null>(null);
  const cancelAllRef = useRef(false);
  const [error, setError] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [view, setView] = useState<"streaming" | "non-streaming">(() => {
    const active = catalog.find((m) => m.id === settings?.stt_model);
    return active?.streaming ? "streaming" : "non-streaming";
  });

  const isModelDownloading = (id: string) =>
    startingDownloads.has(id) || modelProgress.some((p) => p.file === id);

  const isModelCancelling = (id: string) => cancellingDownloads.has(id);

  const anyDownloading = modelProgress.length > 0 || startingDownloads.size > 0;

  // The initial state above runs before the catalog is loaded (it starts as
  // []), so the tab defaulted to "non-streaming" even when the active model
  // was streaming. Re-sync once the catalog/settings arrive.
  useEffect(() => {
    const active = catalog.find((m) => m.id === settings?.stt_model);
    if (active) setView(active.streaming ? "streaming" : "non-streaming");
  }, [catalog, settings?.stt_model]);

  // Once a progress event arrives for a model, clear it from the pending/starting set.
  useEffect(() => {
    if (startingDownloads.size === 0) return;
    setStartingDownloads((prev) => {
      let changed = false;
      const next = new Set(prev);
      for (const id of prev) {
        if (modelProgress.some((p) => p.file === id)) {
          next.delete(id);
          changed = true;
        }
      }
      return changed ? next : prev;
    });
  }, [modelProgress, startingDownloads]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;

    api.onModelError((payload) => {
      setStartingDownloads((prev) => {
        const next = new Set(prev);
        next.delete(payload.file);
        return next;
      });
      setError(payload.error);
      toast.error(`Model download failed: ${payload.error}`);
    }).then((fn) => {
      if (cancelled) {
        fn();
      } else {
        unlisten = fn;
      }
    });

    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
  }, []);

  const handleDownload = async (id: string) => {
    setStartingDownloads((prev) => new Set(prev).add(id));
    setError(null);
    try {
      await api.ensureModel(id);
    } catch (e) {
      setStartingDownloads((prev) => {
        const next = new Set(prev);
        next.delete(id);
        return next;
      });
      setError(String(e));
      toast.error(`Failed to start download: ${String(e)}`);
    }
  };

  const handleCancelDownload = async (id: string) => {
    setCancellingDownloads((prev) => new Set(prev).add(id));
    try {
      await api.cancelModelDownload(id);
      useStore.getState().removeModelProgress(id);
      setStartingDownloads((prev) => {
        const next = new Set(prev);
        next.delete(id);
        return next;
      });
      cancelAllRef.current = true;
      setDownloadAllStatus(null);
      toast.info("Download cancelled");
      await useStore.getState().refreshCatalog();
    } catch (e) {
      toast.error(`Could not cancel download: ${String(e)}`);
    } finally {
      setCancellingDownloads((prev) => {
        const next = new Set(prev);
        next.delete(id);
        return next;
      });
    }
  };

  const handleDownloadAll = async () => {
    const missing = visibleModels.filter((m) => m.available && !m.installed);
    if (missing.length === 0) return;
    cancelAllRef.current = false;
    setError(null);
    for (let i = 0; i < missing.length; i++) {
      if (cancelAllRef.current) break;
      const model = missing[i];
      setDownloadAllStatus({ current: i + 1, total: missing.length, name: model.name });
      try {
        await handleDownload(model.id);
        // Wait until the model is installed or cancelled before starting next
        await new Promise<void>((resolve) => {
          const check = setInterval(async () => {
            if (cancelAllRef.current) {
              clearInterval(check);
              resolve();
              return;
            }
            const cat = await api.getModelsCatalog().catch(() => []);
            const m = cat.find((item) => item.id === model.id);
            const inProgress = useStore.getState().modelProgress.some((p) => p.file === model.id);
            if (m?.installed || (!inProgress && !startingDownloads.has(model.id))) {
              clearInterval(check);
              resolve();
            }
          }, 500);
        });
      } catch (e) {
        if (!cancelAllRef.current) {
          setError(String(e));
          toast.error(`Failed downloading ${model.name}: ${String(e)}`);
        }
        setDownloadAllStatus(null);
        return;
      }
    }
    setDownloadAllStatus(null);
    await useStore.getState().refreshAll();
  };

  const handleCancelAll = async () => {
    cancelAllRef.current = true;
    setDownloadAllStatus(null);
    const activeFiles = useStore.getState().modelProgress.map((p) => p.file);
    for (const id of activeFiles) {
      await handleCancelDownload(id).catch(() => {});
    }
    for (const id of startingDownloads) {
      await handleCancelDownload(id).catch(() => {});
    }
  };

  const handleUse = async (modelId: string, engineKey: string) => {
    try {
      await api.setSettings({ engine: engineKey, stt_model: modelId });
      await useStore.getState().refreshAll();
      api.warmupModel(engineKey).catch(() => {});
    } catch (e) {
      setError(String(e));
    }
  };

  const handleDelete = async (id: string) => {
    const isModelInUse = settings?.stt_model === id;
    const ok = await confirmDialog({
      title: isModelInUse ? "Delete active model in use?" : "Delete model?",
      description: isModelInUse
        ? `“${id}” is currently selected for dictation. If deleted, you will need to choose another model before dictating.`
        : `“${id}” will be removed from disk.`,
      confirmLabel: "Delete",
      destructive: true,
    });
    if (!ok) return;
    try {
      await api.removeModel(id);
      toast.success(`Deleted ${id}`);
      await useStore.getState().refreshAll();
    } catch (e) {
      toast.error(String(e));
    }
  };

  const installedBytes = catalog
    .filter((m) => m.installed)
    .reduce((sum, m) => sum + (Number.isFinite(m.disk_bytes) ? m.disk_bytes : 0), 0);
  const sttModels = catalog.filter((m) => m.kind === "stt");
  const captionModel = catalog.find((m) => m.kind === "caption");
  const captionProgress = captionModel ? modelProgress.find((p) => p.file === captionModel.id) : undefined;
  const captionBusy = captionModel ? isModelDownloading(captionModel.id) : false;
  const captionCancelling = captionModel ? isModelCancelling(captionModel.id) : false;
  const q = searchQuery.trim().toLowerCase();
  const tabModels = sttModels.filter((m) => m.streaming === (view === "streaming"));
  const visibleModels = q
    ? tabModels.filter(
        (m) =>
          m.name.toLowerCase().includes(q) ||
          m.id.toLowerCase().includes(q) ||
          (m.engine_key ?? "").toLowerCase().includes(q)
      )
    : tabModels;
  const missingCount = visibleModels.filter((m) => m.available && !m.installed).length;
  const otherTab = view === "streaming" ? "non-streaming" : "streaming";
  const otherTabMatches = q
    ? sttModels
        .filter((m) => m.streaming === (otherTab === "streaming"))
        .filter(
          (m) =>
            m.name.toLowerCase().includes(q) ||
            m.id.toLowerCase().includes(q) ||
            (m.engine_key ?? "").toLowerCase().includes(q)
        ).length
    : 0;

  return (
    <div className="flex flex-col gap-4">
      <div className="relative flex items-center">
        <Input
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          placeholder="Filter models by name, id, or engine..."
          className="pr-8"
        />
        {searchQuery && (
          <button
            type="button"
            onClick={() => setSearchQuery("")}
            className="absolute right-2.5 flex size-5 items-center justify-center text-xs font-bold text-muted-foreground hover:text-foreground cursor-pointer"
            aria-label="Clear search"
          >
            ✕
          </button>
        )}
      </div>

      <div className="flex border-2 border-border">
        {(["streaming", "non-streaming"] as const).map((tab) => (
          <button
            key={tab}
            onClick={() => setView(tab)}
            className={`min-w-0 flex-1 truncate border-border px-2 py-2 text-xs font-bold tracking-widest uppercase ${
              view === tab
                ? "bg-primary text-primary-foreground"
                : "bg-card text-foreground hover:bg-accent"
            } ${tab === "non-streaming" ? "border-l-2" : ""}`}
          >
             {SECTION_LABELS[tab]}
          </button>
        ))}
      </div>

      {(tabModels.length > 0 || (searchQuery.trim() && catalog.length > 0)) && (
        <div className="flex flex-col gap-2">
          <div className="flex flex-col">
            <h3 className="text-xs font-bold uppercase tracking-widest">
              Speech-to-text engines
            </h3>
            <p className="mb-2 text-xs text-muted-foreground">{SECTION_HINTS[view]}</p>
          </div>
          {visibleModels.length > 0 ? (
            <div className="flex flex-col">
              {visibleModels.map((model, i) => {
                const progress = modelProgress.find((p) => p.file === model.id);
                const isActive =
                  model.engine_key != null &&
                  settings?.engine === model.engine_key &&
                  settings?.stt_model === model.id;
                const busy = isModelDownloading(model.id);
                const cancelling = isModelCancelling(model.id);
                const size = modelSize(model);
                return (
                  <div
                    key={model.id}
                    className={`flex flex-col gap-2 border-2 p-3 ${
                      model.available
                        ? isActive
                          ? "border-border bg-primary text-primary-foreground"
                          : "border-border bg-card"
                        : "border-muted bg-muted"
                    } ${i > 0 ? "border-t-0" : ""}`}
                  >
                    <div className="flex min-w-0 flex-wrap items-center gap-2">
                      <span
                        className={`flex size-5 shrink-0 items-center justify-center border-2 text-[10px] font-bold ${
                          model.available
                            ? isActive
                              ? "border-primary-foreground bg-primary-foreground text-primary"
                              : "border-primary bg-primary text-primary-foreground"
                            : "border-muted-foreground/50 text-muted-foreground"
                        }`}
                      >
                        ASR
                      </span>
                      <span className="text-sm font-bold tracking-wide uppercase">
                        {model.name}
                      </span>
                      {model.id === "nemo-streaming-fastconformer-ctc-en-80ms" && (
                        <span className="border border-amber-500/50 bg-amber-500/10 px-2 py-0.5 text-[10px] font-bold text-amber-500 uppercase tracking-wider">
                          ★ Recommended (Streaming Speed)
                        </span>
                      )}
                      {model.id === "parakeet-tdt-ctc-110m-int8" && (
                        <span className="border border-emerald-500/50 bg-emerald-500/10 px-2 py-0.5 text-[10px] font-bold text-emerald-500 uppercase tracking-wider">
                          ★ Recommended (Default / Balanced)
                        </span>
                      )}
                      {model.id === "whisper-turbo-en" && (
                        <span className="border border-sky-500/50 bg-sky-500/10 px-2 py-0.5 text-[10px] font-bold text-sky-500 uppercase tracking-wider">
                          ★ Recommended (High Accuracy)
                        </span>
                      )}
                      <span
                        className={`ml-auto min-w-0 truncate text-[11px] font-bold tracking-wider uppercase tabular-nums ${
                          isActive ? "text-primary-foreground/70" : "text-muted-foreground"
                        }`}
                      >
                        {size !== null
                          ? `${formatBytes(size)}${model.installed ? " on disk" : " download"}`
                          : "size unknown"}
                      </span>
                    </div>
                    <div className="flex min-w-0 flex-wrap items-center gap-2">
                      {model.installed ? (
                        <>
                          <span
                            className={`border px-1.5 py-0.5 text-[10px] font-bold tracking-wider uppercase ${
                              isActive
                                ? "border-primary-foreground text-primary bg-primary-foreground"
                                : "border-primary bg-primary text-primary-foreground"
                            }`}
                          >
                            ✓ Installed
                          </span>
                          <Button
                            size="sm"
                            variant="ghost"
                            className={isActive ? "text-primary-foreground hover:bg-primary-foreground/20 hover:text-primary-foreground" : "text-muted-foreground"}
                            onClick={() => handleDelete(model.id)}
                          >
                            Delete
                          </Button>
                        </>
                      ) : model.available ? (
                        busy ? (
                          <Button
                            size="sm"
                            variant="destructive"
                            disabled={cancelling}
                            onClick={() => handleCancelDownload(model.id)}
                          >
                            {cancelling ? "Cancelling…" : "✕ Cancel"}
                          </Button>
                        ) : (
                          <Button
                            size="sm"
                            variant={isActive ? "outline" : "default"}
                            className={isActive ? "border-primary-foreground text-primary-foreground shadow-none" : ""}
                            onClick={() => handleDownload(model.id)}
                            disabled={anyDownloading}
                          >
                            Download
                          </Button>
                        )
                      ) : (
                        <span className="text-[10px] font-bold tracking-wider uppercase text-muted-foreground">
                          Coming soon
                        </span>
                      )}
                      {model.installed && (
                        model.engine_key != null && isActive ? (
                          <span className="ml-auto animate-od-blink border border-primary-foreground px-1.5 py-0.5 text-[10px] font-bold tracking-wider uppercase">
                            In use
                          </span>
                        ) : streamingTooSlow && model.streaming ? (
                          <span
                            className="ml-auto border border-destructive px-1.5 py-0.5 text-[10px] font-bold tracking-wider uppercase text-destructive"
                            title={`This model decodes ~${((modelsStatus?.streaming_rtf_x100 ?? 0) / 100).toFixed(1)}x slower than real time on your CPU — dictation results would be delayed. Pick a non-streaming model instead.`}
                          >
                            Too slow for this CPU
                          </span>
                        ) : (
                          <Button
                            size="sm"
                            variant="ghost"
                            className="ml-auto"
                            onClick={() => handleUse(model.id, model.engine_key!)}
                          >
                            Use
                          </Button>
                        )
                      )}
                    </div>
                    {progress ? (
                      <div className="flex flex-col gap-1">
                        <div className="flex items-center justify-between text-[10px] font-bold uppercase tracking-wider tabular-nums">
                          <span>
                            Downloading…
                            {progress.speedBytesPerSec && progress.speedBytesPerSec > 0
                              ? ` (${formatBytes(progress.speedBytesPerSec)}/s)`
                              : ""}
                          </span>
                          <span>
                            {formatBytes(progress.received)}
                            {progress.total > 0
                              ? ` / ${formatBytes(progress.total)}`
                              : " so far"}
                            {progress.etaSeconds != null && progress.etaSeconds > 0
                              ? ` · ETA ${progress.etaSeconds < 60 ? `${progress.etaSeconds}s` : `${Math.floor(progress.etaSeconds / 60)}m ${progress.etaSeconds % 60}s`}`
                              : ""}
                          </span>
                        </div>
                        <Progress
                          value={progressPercent(progress.received, progress.total)}
                          className="w-full"
                        />
                      </div>
                    ) : busy ? (
                      <div className="flex flex-col gap-1">
                        <div className="flex items-center justify-between text-[10px] font-bold uppercase tracking-wider">
                          <span className="flex items-center gap-1.5">
                            <Loader2 className="size-3 animate-spin" />
                            {cancelling ? "Cancelling…" : "Connecting…"}
                          </span>
                        </div>
                        <div className="h-4 w-full border-2 border-border bg-background overflow-hidden">
                          <div className="h-full w-full bg-primary/20 animate-od-shimmer" />
                        </div>
                      </div>
                    ) : null}
                  </div>
                );
              })}
            </div>
          ) : (
            <div className="flex flex-col items-center justify-center border-2 border-dashed border-border p-6 text-center">
              <p className="text-xs text-muted-foreground">
                No models match “<span className="font-semibold text-foreground">{searchQuery}</span>”
              </p>
              <div className="mt-3 flex items-center gap-2">
                <Button
                  variant="outline"
                  size="sm"
                  className="text-xs"
                  onClick={() => setSearchQuery("")}
                >
                  Clear search
                </Button>
                {otherTabMatches > 0 && (
                  <Button
                    variant="ghost"
                    size="sm"
                    className="text-xs"
                    onClick={() => setView(otherTab)}
                  >
                    View in {SECTION_LABELS[otherTab]} ({otherTabMatches})
                  </Button>
                )}
              </div>
            </div>
          )}
        </div>
      )}

      {modelsStatus?.gpu_active && (
        <span className="w-fit border-2 border-primary bg-primary px-1.5 py-0.5 text-[10px] font-bold tracking-wider uppercase text-primary-foreground">
          GPU ✓ {modelsStatus.gpu_mode}
        </span>
      )}
      {captionModel && (
        <div className="flex flex-col gap-2 border-2 border-dashed border-border bg-card p-3">
          <div className="flex min-w-0 flex-wrap items-center gap-2">
            <span className="border border-border px-1.5 py-0.5 text-[10px] font-bold tracking-wider uppercase text-muted-foreground">
              Built-in
            </span>
            <span className="text-sm font-bold uppercase tracking-wide">{captionModel.name}</span>
            {captionModel.installed ? (
              <span className="border border-border bg-primary px-1.5 py-0.5 text-[10px] font-bold tracking-wider uppercase text-primary-foreground">
                ✓ Ready
              </span>
            ) : captionBusy ? (
              <Button
                size="sm"
                variant="destructive"
                className="ml-auto"
                title="Cancel download"
                disabled={captionCancelling}
                onClick={() => handleCancelDownload(captionModel.id)}
              >
                {captionCancelling ? "Cancelling…" : "✕ Cancel"}
              </Button>
            ) : (
              <span className="ml-auto">
                <Button size="sm" onClick={() => handleDownload(captionModel.id)} disabled={anyDownloading}>
                  Download
                </Button>
              </span>
            )}
          </div>
          {captionProgress ? (
            <div className="flex flex-col gap-1">
              <div className="flex items-center justify-between text-[10px] font-bold uppercase tracking-wider tabular-nums">
                <span>
                  Downloading…
                  {captionProgress.speedBytesPerSec && captionProgress.speedBytesPerSec > 0
                    ? ` (${formatBytes(captionProgress.speedBytesPerSec)}/s)`
                    : ""}
                </span>
                <span>
                  {formatBytes(captionProgress.received)}
                  {captionProgress.total > 0
                    ? ` / ${formatBytes(captionProgress.total)}`
                    : " so far"}
                  {captionProgress.etaSeconds != null && captionProgress.etaSeconds > 0
                    ? ` · ETA ${captionProgress.etaSeconds < 60 ? `${captionProgress.etaSeconds}s` : `${Math.floor(captionProgress.etaSeconds / 60)}m ${captionProgress.etaSeconds % 60}s`}`
                    : ""}
                </span>
              </div>
              <Progress
                value={progressPercent(captionProgress.received, captionProgress.total)}
                className="w-full"
              />
            </div>
          ) : captionBusy ? (
            <div className="flex flex-col gap-1">
              <div className="flex items-center justify-between text-[10px] font-bold uppercase tracking-wider">
                <span className="flex items-center gap-1.5">
                  <Loader2 className="size-3 animate-spin" />
                  {captionCancelling ? "Cancelling…" : "Connecting…"}
                </span>
              </div>
              <div className="h-4 w-full border-2 border-border bg-background overflow-hidden">
                <div className="h-full w-full bg-primary/20 animate-od-shimmer" />
              </div>
            </div>
          ) : null}
          <p className="text-xs text-muted-foreground">
            Powers live captions during dictation — works with every model. Auto-fetched in the
            background; safe to delete (it re-downloads on demand).
          </p>
        </div>
      )}

      <div className="flex flex-wrap items-center justify-between gap-2">
        <p className="min-w-0 text-xs text-muted-foreground">
          {installedBytes > 0 ? `${formatBytes(installedBytes)} of model storage used` : "No models installed yet."}
        </p>
        {missingCount > 0 && (
          <div className="flex items-center gap-2">
            {downloadAllStatus ? (
              <div className="flex items-center gap-2">
                <span className="text-xs font-bold text-muted-foreground uppercase">
                  Downloading {downloadAllStatus.current} of {downloadAllStatus.total} ({downloadAllStatus.name})…
                </span>
                <Button size="sm" variant="destructive" onClick={handleCancelAll}>
                  Cancel all
                </Button>
              </div>
            ) : (
              <Button size="sm" variant="outline" onClick={handleDownloadAll} disabled={anyDownloading}>
                Download all missing ({missingCount})
              </Button>
            )}
          </div>
        )}
      </div>

      {error && (
        <div className="border-2 border-primary bg-primary px-2 py-1.5 text-xs font-bold text-primary-foreground uppercase">
          ✕ {error}
        </div>
      )}
      <p className="text-xs text-muted-foreground">
        Everything runs 100% offline. Installed models show their real size on disk and can be
        deleted anytime.
      </p>
    </div>
  );
}
