import { useStore } from "@/lib/store";
import { ModelCard } from "@/components/ModelCard";
import { Badge } from "@/components/ui/badge";
import { HardDrive, CheckCircle2, Info } from "lucide-react";

function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "0 MB";
  if (bytes < 1024) return `${Math.round(bytes)} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

export function ModelsTab() {
  const catalog = useStore((s) => s.catalog);
  const settings = useStore((s) => s.settings);
  const modelsStatus = useStore((s) => s.models);

  const installedModels = catalog.filter((m) => m.installed);
  const totalInstalledBytes = installedModels.reduce(
    (sum, m) => sum + (Number.isFinite(m.disk_bytes) ? m.disk_bytes : 0),
    0
  );

  const activeModel = catalog.find((m) => m.id === settings?.stt_model);
  const activeGpuMode = settings?.gpu ?? modelsStatus?.gpu_mode ?? "auto";

  return (
    <div className="flex flex-col gap-6">
      {/* Top Banner / Summary */}
      <div className="flex flex-col gap-3 rounded-lg border border-border bg-card p-4">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="flex items-center gap-2.5">
            
            <div>
              <h2 className="text-sm font-semibold ">Speech models</h2>
              <p className="text-xs text-muted-foreground">
                Everything is transcribed on this device. Choose for live captions or for accuracy.
              </p>
            </div>
          </div>

          <div className="flex flex-wrap items-center gap-2">
            <Badge variant="outline" className="flex items-center gap-1.5 px-2 py-1 text-[11px] font-semibold ">
              <CheckCircle2 className="size-3.5 text-primary" />
              Active: {activeModel?.name ?? settings?.stt_model ?? "None"}
            </Badge>
            <Badge variant="outline" className="flex items-center gap-1.5 px-2 py-1 text-[11px] font-semibold ">
              <HardDrive className="size-3.5 text-muted-foreground" />
              {installedModels.length} installed ({formatBytes(totalInstalledBytes)})
            </Badge>
            <Badge variant="outline" className="flex items-center gap-1.5 px-2 py-1 text-[11px] font-semibold ">
              GPU: {activeGpuMode.toUpperCase()}
            </Badge>
          </div>
        </div>
      </div>

      {/* Model Loading / Cold Start Disclaimer */}
      <div className="flex items-center gap-2.5 rounded-lg bg-muted px-3.5 py-2.5 text-xs text-muted-foreground">
        <Info className="size-4 shrink-0 text-muted-foreground" />
        <span>
          The first transcription after opening the app is slower while the model loads. After that it is fast.
        </span>
      </div>

      {/* Model Catalog & Actions */}
      <ModelCard />
    </div>
  );
}
