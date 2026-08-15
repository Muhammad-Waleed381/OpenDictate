import { useEffect, useState } from "react";
import { useStore } from "@/lib/store";
import * as api from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function ModelCard() {
  const catalog = useStore((s) => s.catalog);
  const settings = useStore((s) => s.settings);
  const modelProgress = useStore((s) => s.modelProgress);
  const [downloading, setDownloading] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

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

  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-col">
        {catalog.map((model, i) => {
          const progress = modelProgress.find((p) => p.file === model.id);
          const isActive =
            model.kind === "stt" &&
            model.engine_key != null &&
            settings?.engine === model.engine_key &&
            settings?.stt_model === model.id;
          const busy = downloading === model.id;
          const size = model.installed && model.disk_bytes > 0 ? model.disk_bytes : model.size_bytes;
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
                  {model.kind === "stt" ? "ASR" : "VAD"}
                </span>
                <span className="flex-1 truncate text-sm font-bold tracking-wide uppercase">
                  {model.name}
                </span>
                <span
                  className={`shrink-0 text-[11px] font-bold tracking-wider uppercase tabular-nums ${
                    isActive ? "text-white/70" : "text-muted-foreground"
                  }`}
                >
                  {formatBytes(size)}
                  {model.installed && model.disk_bytes > 0 ? " on disk" : " download"}
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
                {model.kind === "stt" &&
                  model.engine_key != null &&
                  model.installed &&
                  (isActive ? (
                    <span className="ml-auto animate-od-blink border border-white px-1.5 py-0.5 text-[10px] font-bold tracking-wider uppercase">
                      In use
                    </span>
                  ) : (
                    <Button
                      size="sm"
                      variant={isActive ? "outline" : "ghost"}
                      className="ml-auto"
                      onClick={() => handleUse(model.id, model.engine_key!)}
                    >
                      Use
                    </Button>
                  ))}
                {model.kind === "vad" && model.installed && (
                  <span className="ml-auto text-[10px] font-bold tracking-wider uppercase text-white/70">
                    Always on
                  </span>
                )}
              </div>
              {progress && (
                <div className="flex flex-col gap-1">
                  <div className="flex items-center justify-between text-[10px] font-bold uppercase tracking-wider tabular-nums">
                    <span>Downloading…</span>
                    <span>
                      {formatBytes(progress.received)} /{" "}
                      {formatBytes(progress.total)}
                    </span>
                  </div>
                  <Progress
                    value={
                      progress.total > 0
                        ? Math.round((progress.received / progress.total) * 100)
                        : 0
                    }
                    className="w-full"
                  />
                </div>
              )}
            </div>
          );
        })}
      </div>
      {error && (
        <div className="border-2 border-black bg-black px-2 py-1.5 text-xs font-bold text-white uppercase">
          ✕ {error}
        </div>
      )}
      <p className="text-xs text-muted-foreground">
        Download a model, then hit{" "}
        <span className="font-bold">Use</span> to make it the active engine.
        Installed models show their real size on disk and can be deleted.
        Everything runs 100% offline.
      </p>
    </div>
  );
}