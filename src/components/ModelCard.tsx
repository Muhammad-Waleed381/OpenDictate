import { useEffect, useState } from "react";
import { useStore } from "@/lib/store";
import * as api from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { toast } from "@/components/ui/toast";
import { confirmDialog } from "@/components/ui/confirm-dialog";

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
  const [downloading, setDownloading] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [view, setView] = useState<"streaming" | "non-streaming">(() => {
    const active = catalog.find((m) => m.id === settings?.stt_model);
    return active?.streaming ? "streaming" : "non-streaming";
  });

  useEffect(() => {
    if (!downloading) return;
    if (modelProgress.length === 0) {
      setDownloading(null);
      useStore.getState().refreshCatalog().catch(() => {});
    }
  }, [modelProgress, downloading]);

  const handleDownload = async (id: string) => {
    setDownloading(id);
    setError(null);
    try {
      await api.ensureModel(id);
    } catch (e) {
      setDownloading(null);
      setError(String(e));
    }
  };

  const handleDownloadAll = async () => {
    const missing = visibleModels.filter((m) => m.available && !m.installed);
    if (missing.length === 0) return;
    setError(null);
    for (const model of missing) {
      setDownloading(model.id);
      try {
        await api.ensureModel(model.id);
      } catch (e) {
        setDownloading(null);
        setError(String(e));
        return;
      }
    }
    setDownloading(null);
    await useStore.getState().refreshAll();
  };

  const handleUse = async (modelId: string, engineKey: string) => {
    try {
      await api.setSettings({ engine: engineKey, stt_model: modelId });
      await useStore.getState().refreshAll();
      api.warmupModel(modelId, engineKey).catch(() => {});
    } catch (e) {
      setError(String(e));
    }
  };

  const handleDelete = async (id: string) => {
    const ok = await confirmDialog({
      title: "Delete model?",
      description: `${id} will be removed from disk.`,
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
  const captionBusy = downloading === captionModel?.id;
  const visibleModels = sttModels.filter((m) => m.streaming === (view === "streaming"));
  const missingCount = visibleModels.filter((m) => m.available && !m.installed).length;

  return (
    <div className="flex flex-col gap-4">
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

      {visibleModels.length > 0 && (
        <div className="flex flex-col gap-2">
          <div className="flex flex-col">
            <h3 className="text-xs font-bold uppercase tracking-widest">
              Speech-to-text engines
            </h3>
            <p className="mb-2 text-xs text-muted-foreground">{SECTION_HINTS[view]}</p>
          </div>
          <div className="flex flex-col">
            {visibleModels.map((model, i) => {
              const progress = modelProgress.find((p) => p.file === model.id);
              const isActive =
                model.engine_key != null &&
                settings?.engine === model.engine_key &&
                settings?.stt_model === model.id;
              const busy = downloading === model.id;
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
                  <div className="flex min-w-0 items-center gap-2">
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
                    <span className="flex-1 truncate text-sm font-bold tracking-wide uppercase">
                      {model.name}
                    </span>
                    <span
                      className={`min-w-0 truncate text-[11px] font-bold tracking-wider uppercase tabular-nums ${
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
                      <Button
                        size="sm"
                        variant={isActive ? "outline" : "default"}
                        className={isActive ? "border-primary-foreground text-primary-foreground shadow-none" : ""}
                        onClick={() => handleDownload(model.id)}
                        disabled={busy}
                      >
                        {busy ? "Downloading…" : "Download"}
                      </Button>
                    ) : (
                      <span className="text-[10px] font-bold tracking-wider uppercase text-muted-foreground">
                        Coming soon
                      </span>
                    )}
                    {model.engine_key != null &&
                      model.installed &&
                      isActive ? (
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
                      )}
                  </div>
                  {progress && (
                    <div className="flex flex-col gap-1">
                      <div className="flex items-center justify-between text-[10px] font-bold uppercase tracking-wider tabular-nums">
                        <span>Downloading…</span>
                        <span>
                          {formatBytes(progress.received)}
                          {progress.total > 0
                            ? ` / ${formatBytes(progress.total)}`
                            : " so far"}
                        </span>
                      </div>
                      <Progress
                        value={progressPercent(progress.received, progress.total)}
                        className="w-full"
                      />
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        </div>
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
            ) : (
              <span className="ml-auto">
                <Button size="sm" onClick={() => handleDownload(captionModel.id)} disabled={captionBusy}>
                  {captionBusy ? "Downloading…" : "Download"}
                </Button>
              </span>
            )}
            {captionBusy && (
              <Button size="icon-sm" variant="ghost" title="Cancel" onClick={() => handleDelete(captionModel.id)}>
                ×
              </Button>
            )}
          </div>
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
          <Button size="sm" variant="outline" onClick={handleDownloadAll} disabled={downloading !== null}>
            {downloading !== null ? "Downloading…" : `Download all missing (${missingCount})`}
          </Button>
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
