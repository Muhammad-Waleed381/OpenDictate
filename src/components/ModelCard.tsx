import { useEffect, useState } from "react";
import { useStore } from "@/lib/store";
import * as api from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Progress } from "@/components/ui/progress";

function fileName(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] || path;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function ModelCard() {
  const models = useStore((s) => s.models);
  const modelProgress = useStore((s) => s.modelProgress);
  const [downloading, setDownloading] = useState(false);

  const sttReady = models?.stt_ready ?? false;
  const vadReady = models?.vad_ready ?? false;
  const allReady = sttReady && vadReady;

  const handleDownload = async () => {
    setDownloading(true);
    try {
      await api.ensureModels();
      await useStore.getState().refreshModels();
    } catch {
      setDownloading(false);
    }
  };

  useEffect(() => {
    if (modelProgress.length === 0) setDownloading(false);
  }, [modelProgress]);

  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-center gap-2">
        <span className="text-sm font-bold uppercase tracking-wider">Models</span>
        <Badge variant={sttReady ? "default" : "outline"}>
          STT {sttReady ? "✓ ready" : "missing"}
        </Badge>
        <Badge variant={vadReady ? "default" : "outline"}>
          VAD {vadReady ? "✓ ready" : "missing"}
        </Badge>
      </div>
      {allReady ? (
        <p className="text-sm">
          Speech-to-text and voice-activity models are installed locally.{" "}
          <span className="font-bold">0 bytes leave your machine.</span>
        </p>
      ) : (
        <div className="flex flex-col gap-3">
          <Button onClick={handleDownload} disabled={downloading} className="w-fit">
            {downloading ? "Downloading…" : "Download models"}
          </Button>
          {modelProgress.length > 0 && (
            <div className="flex flex-col gap-2">
              {modelProgress.map((p) => (
                <div key={p.file} className="flex flex-col gap-1">
                  <div className="flex items-center justify-between text-xs font-bold uppercase tracking-wider">
                    <span>{fileName(p.file)}</span>
                    <span className="tabular-nums">
                      {formatBytes(p.received)} / {formatBytes(p.total)}
                    </span>
                  </div>
                  <Progress
                    value={
                      p.total > 0 ? Math.round((p.received / p.total) * 100) : 0
                    }
                    className="w-full"
                  />
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}