import { useEffect, useState } from "react";
import { useStore } from "@/lib/store";
import * as api from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";

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

const SECTION_HINTS: Record<"streaming" | "offline", string> = {
  streaming:
    "Streaming engines transcribe live as you speak, with captions — no silence waiting.",
  offline:
    "Offline engines transcribe after you stop speaking. Higher accuracy, no live captions.",
};

export function ModelCard() {
  const catalog = useStore((s) => s.catalog);
  const settings = useStore((s) => s.settings);
  const modelProgress = useStore((s) => s.modelProgress);
  const [downloading, setDownloading] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [view, setView] = useState<"streaming" | "offline">(() => {
    const active = catalog.find((m) => m.id === settings?.stt_model);
    return active?.streaming ? "streaming" : "offline";
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
    } catch (e) {
      setError(String(e));
    }
  };

  const handleDelete = async (id: string) => {
    if (!window.confirm(`Delete ${id} from disk?`)) return;
    try {
      await api.removeModel(id);
      await useStore.getState().refreshAll();
    } catch (e) {
      setError(String(e));
    }
  };

  const installedBytes = catalog
    .filter((m) => m.installed)
    .reduce((sum, m) => sum + (Number.isFinite(m.disk_bytes) ? m.disk_bytes : 0), 0);
  const sttModels = catalog.filter((m) => m.kind === "stt");
  const visibleModels = sttModels.filter((m) => m.streaming === (view === "streaming"));
  const missingCount = visibleModels.filter((m) => m.available && !m.installed).length;

  return (
    <div className="flex flex-col gap-4">
      <div className="flex border-2 border-black">
        {(["streaming", "offline"] as const).map((tab) => (
          <button
            key={tab}
            onClick={() => setView(tab)}
            className={`flex-1 border-black px-3 py-2 text-xs font-bold tracking-widest uppercase ${
              view === tab
                ? "bg-black text-white"
                : "bg-white text-black hover:bg-black/10"
            } ${tab === "offline" ? "border-l-2" : ""}`}
          >
            {tab}
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
                        ? "border-black bg-black text-white"
                        : "border-black bg-white"
                      : "border-muted bg-muted"
                  } ${i > 0 ? "border-t-0" : ""}`}
                >
                  <div className="flex items-center gap-2">
                    <span
                      className={`flex size-5 shrink-0 items-center justify-center border-2 text-[10px] font-bold ${
                        model.available
                          ? isActive
                            ? "border-white text-black bg-white"
                            : "border-black bg-black text-white"
                          : "border-muted-foreground/50 text-muted-foreground"
                      }`}
                    >
                      ASR
                    </span>
                    <span className="flex-1 truncate text-sm font-bold tracking-wide uppercase">
                      {model.name}
                    </span>
                    <span
                      className={`shrink-0 text-[11px] font-bold tracking-wider uppercase tabular-nums ${
                        isActive ? "text-white/70" : "text-muted-foreground"
                      }`}
                    >
                      {size !== null
                        ? `${formatBytes(size)}${model.installed ? " on disk" : " download"}`
                        : "size unknown"}
                    </span>
                  </div>
                  <div className="flex items-center gap-2">
                    {model.installed ? (
                      <>
                        <span
                          className={`border px-1.5 py-0.5 text-[10px] font-bold tracking-wider uppercase ${
                            isActive
                              ? "border-white text-black bg-white"
                              : "border-black bg-black text-white"
                          }`}
                        >
                          ✓ Installed
                        </span>
                        <Button
                          size="sm"
                          variant="ghost"
                          className={isActive ? "text-white hover:bg-white/20 hover:text-white" : "text-muted-foreground"}
                          onClick={() => handleDelete(model.id)}
                        >
                          Delete
                        </Button>
                      </>
                    ) : model.available ? (
                      <Button
                        size="sm"
                        variant={isActive ? "outline" : "default"}
                        className={isActive ? "border-white text-white shadow-none" : ""}
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
                      (isActive ? (
                        <span className="ml-auto animate-od-blink border border-white px-1.5 py-0.5 text-[10px] font-bold tracking-wider uppercase">
                          In use
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
                      ))}
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

      <div className="flex items-center justify-between gap-2">
        <p className="text-xs text-muted-foreground">
          {installedBytes > 0 ? `${formatBytes(installedBytes)} of model storage used` : "No models installed yet."}
        </p>
        {missingCount > 0 && (
          <Button size="sm" variant="outline" onClick={handleDownloadAll} disabled={downloading !== null}>
            {downloading !== null ? "Downloading…" : `Download all missing (${missingCount})`}
          </Button>
        )}
      </div>

      {error && (
        <div className="border-2 border-black bg-black px-2 py-1.5 text-xs font-bold text-white uppercase">
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