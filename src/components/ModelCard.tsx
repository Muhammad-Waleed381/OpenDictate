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
    <div className="flex flex-col gap-3 rounded-xl border border-border bg-card p-4">
      <div className="flex items-center gap-2">
        <span className="text-sm font-medium text-[#F8FAFC]">Models</span>
        <Badge
          variant={sttReady ? "default" : "destructive"}
          className={sttReady ? "bg-[#10B981]/15 text-[#10B981]" : ""}
        >
          STT {sttReady ? "ready" : "missing"}
        </Badge>
        <Badge
          variant={vadReady ? "default" : "destructive"}
          className={vadReady ? "bg-[#10B981]/15 text-[#10B981]" : ""}
        >
          VAD {vadReady ? "ready" : "missing"}
        </Badge>
      </div>
      {allReady ? (
        <p className="text-sm text-[#64748B]">
          Speech-to-text and voice-activity models are installed locally.
        </p>
      ) : (
        <div className="flex flex-col gap-2">
          <Button
            onClick={handleDownload}
            disabled={downloading}
            className="w-fit"
          >
            {downloading ? "Downloading…" : "Download models"}
          </Button>
          {modelProgress.length > 0 && (
            <div className="flex flex-col gap-2">
              {modelProgress.map((p) => (
                <div key={p.file} className="flex flex-col gap-1">
                  <div className="flex items-center justify-between text-xs">
                    <span className="text-[#64748B]">{fileName(p.file)}</span>
                    <span className="text-[#64748B]">
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