import { useState } from "react";
import { useStore } from "@/lib/store";
import * as api from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { Progress } from "@/components/ui/progress";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "—";
  if (bytes < 1024) return `${Math.round(bytes)} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function progressPercent(received: number, total: number): number {
  if (!Number.isFinite(received) || !Number.isFinite(total) || total <= 0) return 0;
  return Math.min(100, Math.round((received / total) * 100));
}

const micLabel = (mic: string | null, mics: api.MicDevice[]): string => {
  if (mic === null) return "Select microphone";
  const found = mics.find((m) => m.id === mic);
  return found?.label ?? mic;
};

function SectionTitle({ children }: { children: React.ReactNode }) {
  return (
    <h2 className="text-xs font-bold tracking-widest uppercase text-muted-foreground">
      {children}
    </h2>
  );
}

function ReadyStrip() {
  const mics = useStore((s) => s.mics);
  const mic = useStore((s) => s.mic);
  const settings = useStore((s) => s.settings);
  const catalog = useStore((s) => s.catalog);
  const modelProgress = useStore((s) => s.modelProgress);
  const [downloading, setDownloading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const activeModel = catalog.find((m) => m.id === settings?.stt_model);
  const progress = activeModel
    ? modelProgress.find((p) => p.file === activeModel.id)
    : undefined;
  const micReady = mics.length > 0;
  const modelReady = !!activeModel?.installed;

  const handleMicChange = async (name: string | null) => {
    if (!name) return;
    try {
      await api.setMic(name);
      useStore.getState().setMic(name);
    } catch {}
  };

  const handleDownload = async () => {
    if (!activeModel) return;
    setDownloading(true);
    setError(null);
    try {
      await api.ensureModel(activeModel.id);
      await useStore.getState().refreshAll();
    } catch (e) {
      setError(String(e));
    } finally {
      setDownloading(false);
    }
  };

  return (
    <div className="flex flex-col gap-4">
      <SectionTitle>Ready to dictate?</SectionTitle>
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
        <Card>
          <CardContent className="flex flex-col gap-2 p-3">
          <div className="flex items-center justify-between">
            <Label>Microphone</Label>
            {micReady ? (
              <span className="border border-black bg-black px-1.5 py-0.5 text-[10px] font-bold tracking-wider uppercase text-white">
                ✓ Ready
              </span>
            ) : (
              <span className="border border-black px-1.5 py-0.5 text-[10px] font-bold tracking-wider uppercase">
                Needs setup
              </span>
            )}
          </div>
          <Select value={mic ?? ""} onValueChange={handleMicChange}>
            <SelectTrigger className="w-full">
              <SelectValue>{micLabel(mic, mics)}</SelectValue>
            </SelectTrigger>
            <SelectContent className="w-full">
              {mics.length === 0 ? (
                <SelectItem value="__none__" disabled>
                  No microphones found
                </SelectItem>
              ) : (
                mics.map((device) => (
                  <SelectItem key={device.id} value={device.id}>
                    {device.label}
                  </SelectItem>
                ))
              )}
            </SelectContent>
          </Select>
          </CardContent>
        </Card>

        <Card>
          <CardContent className="flex flex-col gap-2 p-3">
          <div className="flex items-center justify-between">
            <Label>Model</Label>
            {modelReady ? (
              <span className="border border-black bg-black px-1.5 py-0.5 text-[10px] font-bold tracking-wider uppercase text-white">
                ✓ Ready
              </span>
            ) : (
              <span className="border border-black px-1.5 py-0.5 text-[10px] font-bold tracking-wider uppercase">
                Needs download
              </span>
            )}
          </div>
          <div className="flex items-center gap-2">
            <span className="flex-1 truncate text-sm font-bold uppercase tracking-wide">
              {activeModel?.name ?? settings?.stt_model ?? "No model selected"}
            </span>
            {!modelReady && activeModel?.available && (
              <Button size="sm" onClick={handleDownload} disabled={downloading}>
                {downloading ? "Downloading…" : "Download"}
              </Button>
            )}
          </div>
          {!modelReady && activeModel && (
            <p className="text-xs text-muted-foreground">
              {activeModel.available
                ? `Download ${formatBytes(activeModel.size_bytes)} to start dictating.`
                : "Model unavailable."}
            </p>
          )}
          {progress && (
            <Progress
              value={progressPercent(progress.received, progress.total)}
              className="w-full"
            />
          )}
          </CardContent>
        </Card>
      </div>
      {error && (
        <div className="border-2 border-black bg-black px-2 py-1.5 text-xs font-bold text-white uppercase">
          ✕ {error}
        </div>
      )}
    </div>
  );
}

function RecordButton() {
  const recording = useStore((s) => s.recording);
  const [error, setError] = useState<string | null>(null);

  const handleClick = async () => {
    setError(null);
    if (recording) {
      try {
        const result = await api.stopRecording();
        if (result?.text) useStore.setState({ lastResult: result });
      } catch (e) {
        setError(String(e));
      }
      useStore.getState().setRecording(false);
    } else {
      try {
        await api.startRecording("dictate");
        useStore.getState().setRecording(true);
      } catch (e) {
        setError(String(e));
      }
    }
  };

  return (
    <div className="flex flex-col gap-2">
      <Button
        onClick={handleClick}
        variant={recording ? "outline" : "default"}
        className={`h-16 w-full text-lg font-bold tracking-widest uppercase ${
          recording ? "animate-od-blink" : ""
        }`}
      >
        {recording ? "■ Stop" : "● Record"}
      </Button>
      <p className="text-xs text-muted-foreground">
        {recording
          ? "Recording — press the global hotkey to stop."
          : "Dictate — press Record or your global hotkey."}
      </p>
      {error && (
        <div className="w-full border-2 border-black bg-black px-2 py-1.5 text-xs font-bold text-white uppercase">
          ✕ {error}
        </div>
      )}
    </div>
  );
}

function LastResultPanel() {
  const lastResult = useStore((s) => s.lastResult);
  const [undone, setUndone] = useState(false);

  return (
    <div className="flex flex-col gap-2">
      <SectionTitle>Last result</SectionTitle>
      {!lastResult ? (
        <div className="border-2 border-dashed border-black px-4 py-3 text-sm text-muted-foreground">
          Nothing inserted yet — dictate something and it will show up here.
        </div>
      ) : (
        <Card>
          <CardContent className="flex items-center gap-3 p-4">
            <span className="flex h-5 shrink-0 items-center border-2 border-black bg-black px-1.5 text-[10px] font-bold tracking-wider text-white">
              INSERTED ✓
            </span>
          <span className="truncate text-sm font-medium">“{lastResult.text}”</span>
          <Button
            size="sm"
            variant="outline"
            className="ml-auto shrink-0"
            disabled={undone}
            onClick={async () => {
              try {
                await api.undoLastInsert();
                setUndone(true);
              } catch {}
            }}
          >
            {undone ? "Undone" : "Undo"}
          </Button>
          {lastResult.duration_ms > 0 && (
            <span className="ml-auto shrink-0 text-xs font-bold text-muted-foreground tabular-nums">
              {(lastResult.duration_ms / 1000).toFixed(1)}s
            </span>
          )}
          </CardContent>
        </Card>
      )}
    </div>
  );
}

function LiveCaptionsPanel() {
  const partial = useStore((s) => s.partial);
  const settings = useStore((s) => s.settings);
  const catalog = useStore((s) => s.catalog);
  const activeModel = catalog.find((m) => m.id === settings?.stt_model);
  const streaming = !!activeModel?.streaming;

  return (
    <div className="flex flex-col gap-2">
      <SectionTitle>Live captions</SectionTitle>
      <Card className="bg-primary text-primary-foreground">
        <CardContent className="flex items-center gap-3 px-4 py-2.5">
          <span className="flex h-5 shrink-0 items-center border-2 border-white bg-white px-1.5 text-[10px] font-bold tracking-wider text-black">
            LIVE
          </span>
          <span className="size-2 shrink-0 animate-od-blink bg-white" />
          <span className="truncate text-sm font-medium">
            {partial ? `“${partial}”` : "Speak to see live captions…"}
          </span>
        </CardContent>
      </Card>
      {!streaming && (
        <div className="border-2 border-black bg-white px-3 py-2 text-xs font-bold tracking-wider uppercase">
          ⚠ Live captions require a streaming model — switch to the Parakeet
          streaming model in Settings.
        </div>
      )}
    </div>
  );
}

export function HomeTab() {
  return (
    <div className="flex flex-col gap-6">
      <ReadyStrip />
      <RecordButton />
      <LastResultPanel />
      <LiveCaptionsPanel />
    </div>
  );
}