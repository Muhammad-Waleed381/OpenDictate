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

export function NvidiaLogo({ className = "size-3.5" }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      <path d="M8.948 7.37c-.368.188-.8.472-1.28.852C6.46 9.176 5.37 10.74 5.37 12.35c0 1.25.688 2.378 1.83 3.013.25.138.48.232.7.286-.34-.53-.526-1.16-.526-1.832 0-1.895 1.54-3.435 3.435-3.435.534 0 1.036.126 1.487.345.545-.584.97-1.33 1.233-2.18-.846-.66-2.92-1.428-4.59-1.178zm6.54 3.738c-.378.89-1.002 1.62-1.78 2.096.347.45.556 1.01.556 1.62 0 1.44-1.17 2.61-2.61 2.61-.43 0-.834-.108-1.19-.294.49.262 1.05.414 1.65.414 1.89 0 3.43-1.54 3.43-3.43 0-1.13-.55-2.13-1.39-2.75l1.33-.266zm-4.71-6.14c-4.47.53-8.03 4.23-8.32 8.87-.29 4.63 2.76 8.78 7.23 9.87 4.46 1.08 9.07-1.14 10.96-5.27-2.6 3.49-7.01 4.79-11.02 3.25-4-1.54-6.3-5.74-5.46-9.98.84-4.25 4.64-7.37 8.97-7.42 1.23-.01 2.45.24 3.58.74-1.74-.29-4.32-.32-5.94.27v-.33z" />
    </svg>
  );
}

export function OpenAiLogo({ className = "size-3.5" }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      <path d="M22.282 9.821a5.985 5.985 0 0 0-.516-4.91 6.046 6.046 0 0 0-6.51-2.9A6.065 6.065 0 0 0 4.981 4.18a5.985 5.985 0 0 0-3.998 2.9 6.046 6.046 0 0 0 .743 7.097 5.98 5.98 0 0 0 .51 4.911 6.051 6.051 0 0 0 6.515 2.9A5.985 5.985 0 0 0 13.26 24a6.056 6.056 0 0 0 5.772-4.206 5.99 5.99 0 0 0 3.997-2.9 6.056 6.056 0 0 0-.747-7.073zM13.26 22.43a4.476 4.476 0 0 1-2.876-1.04l.141-.081 4.779-2.758a.795.795 0 0 0 .392-.681v-6.737l2.02 1.168a.071.071 0 0 1 .038.052v5.583a4.504 4.504 0 0 1-4.494 4.494zM3.6 18.304a4.47 4.47 0 0 1-.535-3.014l.142.085 4.783 2.759a.771.771 0 0 0 .78 0l5.843-3.369v2.332a.08.08 0 0 1-.033.062L9.74 19.95a4.5 4.5 0 0 1-6.14-1.646zM2.34 8.937a4.485 4.485 0 0 1 2.366-1.973V12.6a.766.766 0 0 0 .388.677l5.815 3.355-2.02 1.168a.076.076 0 0 1-.071 0l-4.83-2.786A4.504 4.504 0 0 1 2.34 8.937zm16.597 3.855l-5.833-3.387L15.119 8.24a.076.076 0 0 1 .071 0l4.83 2.791a4.494 4.494 0 0 1-.676 8.105v-5.678a.79.79 0 0 0-.407-.666zm2.01-3.023l-.141-.085-4.774-2.782a.776.776 0 0 0-.785 0L9.409 10.27V7.934a.08.08 0 0 1 .033-.061l4.84-2.796a4.5 4.5 0 0 1 6.669 4.689v.004zM8.308 14.898L6.289 13.73a.076.076 0 0 1-.038-.057V8.09a4.5 4.5 0 0 1 7.37-3.454l-.142.08L8.7 7.475a.79.79 0 0 0-.392.681zm1.096-3.465l3.447-1.988 3.445 1.988v3.978l-3.445 1.988-3.447-1.988z" />
    </svg>
  );
}

export function MetaLogo({ className = "size-3.5" }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      <path d="M12 9.09c-.93-1.54-2.37-2.49-4.04-2.49-3.22 0-5.96 2.87-5.96 6.4 0 3.51 2.74 6.4 5.96 6.4 1.75 0 3.24-1.04 4.04-2.49.8 1.45 2.29 2.49 4.04 2.49 3.22 0 5.96-2.89 5.96-6.4 0-3.53-2.74-6.4-5.96-6.4-1.67 0-3.11.95-4.04 2.49zm-4.04 8.01c-2.02 0-3.7-1.87-3.7-4.1 0-2.22 1.68-4.1 3.7-4.1 1.71 0 3.19 1.34 3.65 3.14-.38 1.48-1.67 3.06-3.65 3.06zm8.08 0c-1.98 0-3.27-1.58-3.65-3.06.46-1.8 1.94-3.14 3.65-3.14 2.02 0 3.7 1.88 3.7 4.1 0 2.23-1.68 4.1-3.7 4.1z" />
    </svg>
  );
}

export function CohereLogo({ className = "size-3.5" }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      <path d="M12 2C6.477 2 2 6.477 2 12c0 2.414.856 4.629 2.285 6.365l.08.093A9.957 9.957 0 0 0 12 22c5.523 0 10-4.477 10-10S17.523 2 12 2zm-4.8 5.6a4.4 4.4 0 0 1 7.218 3.398c-.01 1.847-1.157 3.447-2.83 4.015A4.402 4.402 0 0 1 7.2 11.2a4.38 4.38 0 0 1 0-3.6z" />
      <circle cx="12" cy="12" r="3.2" />
    </svg>
  );
}

export function CompanyLogo({ company, className = "size-3.5" }: { company: string; className?: string }) {
  const norm = company.trim().toLowerCase();
  if (norm.includes("nvidia")) return <NvidiaLogo className={className} />;
  if (norm.includes("openai") || norm.includes("whisper")) return <OpenAiLogo className={className} />;
  if (norm.includes("meta")) return <MetaLogo className={className} />;
  if (norm.includes("cohere")) return <CohereLogo className={className} />;
  return (
    <svg className={className} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <circle cx="12" cy="12" r="10" />
      <path d="M12 16v-4" />
      <path d="M12 8h.01" />
    </svg>
  );
}

export function ModelCard() {
  const catalog = useStore((s) => s.catalog);
  const settings = useStore((s) => s.settings);
  const modelProgress = useStore((s) => s.modelProgress);
  const modelsStatus = useStore((s) => s.models);
  const recording = useStore((s) => s.recording || s.recordingBusy);
  const startingDownloads = useStore((s) => s.modelDownloads);
  const cancellingDownloads = useStore((s) => s.cancellingModels);
  const [downloadAllStatus, setDownloadAllStatus] = useState<{
    current: number;
    total: number;
    name: string;
  } | null>(null);
  const cancelAllRef = useRef(false);
  const [error, setError] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedCompany, setSelectedCompany] = useState<string | null>(null);
  const [view, setView] = useState<"streaming" | "non-streaming">(() => {
    const active = catalog.find((m) => m.id === settings?.stt_model);
    return active?.streaming ? "streaming" : "non-streaming";
  });

  const isModelDownloading = (id: string) =>
    startingDownloads.includes(id) || modelProgress.some((p) => p.file === id);

  const isModelCancelling = (id: string) => cancellingDownloads.includes(id);

  const anyDownloading = modelProgress.length > 0 || startingDownloads.length > 0;

  // The initial state above runs before the catalog is loaded (it starts as
  // []), so the tab defaulted to "non-streaming" even when the active model
  // was streaming. Re-sync once the catalog/settings arrive.
  useEffect(() => {
    const active = catalog.find((m) => m.id === settings?.stt_model);
    if (active) setView(active.streaming ? "streaming" : "non-streaming");
  }, [catalog, settings?.stt_model]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;

    api.onModelError((payload) => {
      setError(payload.error);
      toast.error(`Model download failed: ${payload.error}`);
    }).then((fn) => {
      if (cancelled) {
        fn();
      } else {
        unlisten = fn;
      }
    }).catch(() => {});

    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
  }, []);

  const handleDownload = async (id: string) => {
    setError(null);
    try {
      await useStore.getState().downloadModel(id);
    } catch (e) {
      setError(String(e));
      toast.error(`Model installation failed: ${String(e)}`);
    }
  };

  const handleCancelDownload = async (id: string) => {
    cancelAllRef.current = true;
    try {
      await useStore.getState().cancelModel(id);
    } catch (e) {
      toast.error(`Could not cancel download: ${String(e)}`);
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
        await useStore.getState().downloadModel(model.id);
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
    const store = useStore.getState();
    const activeFiles = new Set([...store.modelDownloads, ...store.modelProgress.map((p) => p.file)]);
    for (const id of activeFiles) {
      await handleCancelDownload(id).catch(() => {});
    }
  };

  const handleUse = async (modelId: string, engineKey: string) => {
    if (useStore.getState().recording || useStore.getState().recordingBusy || isModelDownloading(modelId)) return;
    try {
      await api.setSettings({ engine: engineKey, stt_model: modelId });
      await useStore.getState().refreshAll();
      api.warmupModel(engineKey).catch(() => {});
    } catch (e) {
      setError(String(e));
    }
  };

  const handleDelete = async (id: string) => {
    if (useStore.getState().recording || useStore.getState().recordingBusy || isModelDownloading(id)) return;
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
    if (useStore.getState().recording || useStore.getState().recordingBusy || isModelDownloading(id)) return;
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

  const COMPANY_ORDER = ["NVIDIA", "OpenAI", "Meta", "Cohere", "Next-gen Kaldi", "Silero"];
  const availableCompanies = Array.from(
    new Set(
      sttModels
        .map((m) => m.company)
        .filter((c): c is string => Boolean(c))
    )
  ).sort((a, b) => {
    const ia = COMPANY_ORDER.indexOf(a);
    const ib = COMPANY_ORDER.indexOf(b);
    if (ia !== -1 && ib !== -1) return ia - ib;
    if (ia !== -1) return -1;
    if (ib !== -1) return 1;
    return a.localeCompare(b);
  });

  const tabModels = sttModels
    .filter((m) => m.streaming === (view === "streaming"))
    .filter((m) => (selectedCompany ? m.company === selectedCompany : true));

  const visibleModels = q
    ? tabModels.filter(
        (m) =>
          m.name.toLowerCase().includes(q) ||
          m.id.toLowerCase().includes(q) ||
          (m.engine_key ?? "").toLowerCase().includes(q) ||
          (m.company ?? "").toLowerCase().includes(q)
      )
    : tabModels;
  const missingCount = visibleModels.filter((m) => m.available && !m.installed).length;
  const otherTab = view === "streaming" ? "non-streaming" : "streaming";
  const otherTabMatches = q
    ? sttModels
        .filter((m) => m.streaming === (otherTab === "streaming"))
        .filter((m) => (selectedCompany ? m.company === selectedCompany : true))
        .filter(
          (m) =>
            m.name.toLowerCase().includes(q) ||
            m.id.toLowerCase().includes(q) ||
            (m.engine_key ?? "").toLowerCase().includes(q) ||
            (m.company ?? "").toLowerCase().includes(q)
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

      {availableCompanies.length > 0 && (
        <div className="flex flex-wrap items-center gap-1.5" role="toolbar" aria-label="Filter by company">
          <button
            type="button"
            onClick={() => setSelectedCompany(null)}
            className={`inline-flex items-center gap-1.5 border px-2.5 py-1 text-xs font-bold tracking-wider uppercase transition-colors cursor-pointer ${
              selectedCompany === null
                ? "border-primary bg-primary text-primary-foreground"
                : "border-border bg-card text-foreground hover:bg-accent"
            }`}
          >
            All
          </button>
          {availableCompanies.map((comp) => {
            const isSelected = selectedCompany === comp;
            return (
              <button
                key={comp}
                type="button"
                onClick={() => setSelectedCompany(isSelected ? null : comp)}
                className={`inline-flex items-center gap-1.5 border px-2.5 py-1 text-xs font-bold tracking-wider uppercase transition-colors cursor-pointer ${
                  isSelected
                    ? "border-primary bg-primary text-primary-foreground"
                    : "border-border bg-card text-foreground hover:bg-accent"
                }`}
              >
                <CompanyLogo company={comp} className="size-3.5 shrink-0" />
                <span>{comp}</span>
              </button>
            );
          })}
        </div>
      )}

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
                      {model.company && (
                        <span
                          className={`inline-flex items-center gap-1 border px-1.5 py-0.5 text-[10px] font-bold tracking-wider uppercase ${
                            isActive
                              ? "border-primary-foreground/40 bg-primary-foreground/15 text-primary-foreground"
                              : "border-border bg-muted/60 text-foreground"
                          }`}
                        >
                          <CompanyLogo company={model.company} className="size-3 shrink-0" />
                          <span>{model.company}</span>
                        </span>
                      )}
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
                            disabled={recording || busy}
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
                        ) : (
                          <Button
                            size="sm"
                            variant="ghost"
                            className="ml-auto"
                            onClick={() => handleUse(model.id, model.engine_key!)}
                            disabled={recording || busy || model.engine_key == null}
                          >
                            Use
                          </Button>
                        )
                      )}
                    </div>
                    {model.id === "parakeet-unified-en-0.6b-int8-streaming-560ms" && !modelsStatus?.gpu_active &&
                      (modelsStatus?.streaming_rtf_x100 ?? 0) > 150 && (
                      <p className="text-xs">
                        The Parakeet CPU benchmark was slower than real time. Live captions may be delayed;
                        other models and GPU performance have not been measured.
                      </p>
                    )}
                    {progress ? (
                      <div className="flex flex-col gap-1">
                        <div className="flex items-center justify-between text-[10px] font-bold uppercase tracking-wider tabular-nums">
                          <span>
                            {cancelling ? "Cancelling…" : progress.total > 0 && progress.received >= progress.total ? "Installing…" : "Downloading…"}
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
                  {captionCancelling ? "Cancelling…" : captionProgress.total > 0 && captionProgress.received >= captionProgress.total ? "Installing…" : "Downloading…"}
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
