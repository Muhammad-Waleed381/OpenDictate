import { useEffect, useRef, useState } from "react";
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
import { useRecording } from "@/lib/useRecording";
import { cn, DEFAULT_HOTKEY, formatHotkey, tailForDisplay } from "@/lib/utils";
import { Check, Mic, Sparkles, Square, Undo2 } from "lucide-react";
import { milestoneCopy, nextMilestone, formatSpan, secondsSaved, wordCount } from "@/lib/delight";
import { toast } from "@/components/ui/toast";

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
  if (mic === null) return "System default";
  const found = mics.find((m) => m.id === mic);
  return found?.label ?? mic;
};

/** RMS is small for ordinary speech; a square root makes the meter readable. */
const meterValue = (level: number) => Math.min(1, Math.sqrt(Math.max(0, level)) * 1.8);

function StatusChip({ ok, children }: { ok: boolean; children: React.ReactNode }) {
  return (
    <span
      className={cn(
        "inline-flex h-5 items-center gap-1 rounded-sm px-1.5 text-xs font-medium",
        ok ? "bg-positive-soft text-positive" : "bg-notice-soft text-notice",
      )}
    >
      {ok && <Check className="size-3" strokeWidth={2.5} aria-hidden />}
      {children}
    </span>
  );
}

function SectionTitle({ children }: { children: React.ReactNode }) {
  return (
    <h2 className="text-sm font-semibold text-foreground">
      {children}
    </h2>
  );
}

function ReadyStrip() {
  const mics = useStore((s) => s.mics);
  const mic = useStore((s) => s.mic);
  const level = useStore((s) => s.level);
  const recording = useStore((s) => s.recording);
  const recordingBusy = useStore((s) => s.recordingBusy);
  const recordingMode = useStore((s) => s.recordingMode);
  const settings = useStore((s) => s.settings);
  const catalog = useStore((s) => s.catalog);
  const modelProgress = useStore((s) => s.modelProgress);
  const modelDownloads = useStore((s) => s.modelDownloads);
  const cancellingModels = useStore((s) => s.cancellingModels);
  const [testing, setTesting] = useState(false);

  useEffect(() => {
    if (recordingMode !== "test" && !recordingBusy) setTesting(false);
  }, [recordingMode, recordingBusy]);

  const testingRef = useRef(false);
  testingRef.current = testing;
  useEffect(() => {
    return () => {
      if (testingRef.current && useStore.getState().recordingMode === "test") {
        useStore.getState().cancelRecording().catch(() => {});
      }
    };
  }, []);

  const activeModel = catalog.find((m) => m.id === settings?.stt_model);
  const downloading = !!activeModel && modelDownloads.includes(activeModel.id);
  const cancelling = !!activeModel && cancellingModels.includes(activeModel.id);
  const progress = activeModel
    ? modelProgress.find((p) => p.file === activeModel.id)
    : undefined;
  const micReady = mics.length > 0;
  const modelReady = !!activeModel?.installed;

  const handleMicChange = async (name: string | null) => {
    if (!name || useStore.getState().recording || useStore.getState().recordingBusy) return;
    try {
      await api.setMic(name);
      useStore.getState().setMic(name);
      toast.success(`Microphone: ${micLabel(name, mics)}`);
    } catch (e) {
      toast.error(`Microphone switch failed: ${String(e)}`);
    }
  };

  const handleTestToggle = async () => {
    if (useStore.getState().recordingBusy) return;
    if (testing) {
      if (useStore.getState().recordingMode !== "test") { setTesting(false); return; }
      try {
        await useStore.getState().stopRecording();
        setTesting(false);
      } catch (e) {
        toast.error(String(e));
        setTesting(useStore.getState().recordingMode === "test");
      }
    } else {
      if (useStore.getState().recording) return;
      setTesting(true);
      testingRef.current = true;
      try {
        if (!await useStore.getState().startRecording("test")) setTesting(false);
      } catch (e) {
        setTesting(false);
        toast.error(String(e));
      }
    }
  };

  const handleDownload = async () => {
    if (!activeModel) return;
    try {
      await useStore.getState().downloadModel(activeModel.id);
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleCancel = async () => {
    if (!activeModel) return;
    try {
      await useStore.getState().cancelModel(activeModel.id);
    } catch (e) {
      toast.error(String(e));
    }
  };

  return (
    <div className="flex flex-col gap-4">
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
        <Card>
          <CardContent className="flex flex-col gap-3 p-4">
          <div className="flex items-center justify-between">
            <Label>Microphone</Label>
            <StatusChip ok={micReady}>{micReady ? "Ready" : "Needs setup"}</StatusChip>
          </div>
          <Select value={mic ?? "default"} onValueChange={handleMicChange} disabled={recording || recordingBusy}>
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
          <div className="flex items-center justify-between">
            <span className="text-[13px] text-muted-foreground">
              {testing ? "Testing input…" : (recording ? "Dictating…" : "Input level")}
            </span>
            <Button
              type="button"
              variant={testing ? "destructive" : "outline"}
              size="sm"
              onClick={handleTestToggle}
              disabled={recordingBusy || (!testing && recording)}
            >
              {testing ? "Stop test" : "Test mic"}
            </Button>
          </div>
          <div className="h-1.5 overflow-hidden rounded-full bg-secondary" aria-hidden>
            <div
              className="h-full rounded-full bg-brand transition-[width] duration-75"
              style={{ width: `${testing || recording ? meterValue(level) * 100 : 0}%` }}
            />
          </div>
          </CardContent>
        </Card>

        <Card>
          <CardContent className="flex flex-col gap-3 p-4">
          <div className="flex items-center justify-between">
            <Label>Model</Label>
            <StatusChip ok={modelReady}>{modelReady ? "Ready" : "Needs download"}</StatusChip>
          </div>
          <div className="flex items-center gap-2">
            <span className="flex-1 truncate text-sm font-medium">
              {activeModel?.name ?? settings?.stt_model ?? "No model selected"}
            </span>
            {!modelReady && activeModel?.available && (
              downloading || progress ? (
                <Button size="sm" variant="destructive" onClick={handleCancel} disabled={cancelling}>
                  {cancelling ? "Cancelling…" : "Cancel"}
                </Button>
              ) : (
                <Button size="sm" onClick={handleDownload} disabled={downloading}>
                  Download
                </Button>
              )
            )}
          </div>
          {modelReady && (
            <p className="text-[13px] text-muted-foreground">
              Transcribes on this device.{" "}
              <a href="#models" className="text-brand underline-offset-4 hover:underline">Change model</a>
            </p>
          )}
          {!modelReady && activeModel && (
            <p className="text-xs text-muted-foreground">
              {activeModel.available
                ? `Download ${formatBytes(activeModel.size_bytes)} to start dictating.`
                : "Model unavailable."}
            </p>
          )}
          {progress && (
            <div className="flex flex-col gap-1">
              <div className="flex items-center justify-between text-[11px] font-semibold tabular-nums">
                <span>
                  {cancelling ? "Cancelling…" : progress.total > 0 && progress.received >= progress.total ? "Installing…" : "Downloading…"}
                  {progress.speedBytesPerSec && progress.speedBytesPerSec > 0
                    ? ` (${formatBytes(progress.speedBytesPerSec)}/s)`
                    : ""}
                </span>
                <span>
                  {formatBytes(progress.received)}
                  {progress.total > 0 ? ` / ${formatBytes(progress.total)}` : ""}
                  {progress.etaSeconds != null && progress.etaSeconds > 0
                    ? ` · ETA ${progress.etaSeconds < 60 ? `${progress.etaSeconds}s` : `${Math.floor(progress.etaSeconds / 60)}m`}`
                    : ""}
                </span>
              </div>
              <Progress
                value={progressPercent(progress.received, progress.total)}
                className="w-full"
              />
            </div>
          )}
          </CardContent>
        </Card>
      </div>
    </div>
  );
}

const STAGE_BARS = 56;

/** A rolling picture of the voice: quiet ticks at rest, the level while recording. */
function useLevelHistory(active: boolean) {
  const [bars, setBars] = useState<number[]>(() => new Array(STAGE_BARS).fill(0));
  useEffect(() => {
    if (!active) {
      setBars(new Array(STAGE_BARS).fill(0));
      return;
    }
    const id = window.setInterval(() => {
      const value = meterValue(useStore.getState().level);
      setBars((previous) => [...previous.slice(1), value]);
    }, 70);
    return () => window.clearInterval(id);
  }, [active]);
  return bars;
}

function Keycaps({ hotkey, tone }: { hotkey: string; tone: "live" | "stage" | "plain" }) {
  return (
    <span className="inline-flex items-center gap-1 align-middle" aria-label={`Global hotkey ${hotkey}`}>
      {hotkey.split("+").map((key) => (
        <kbd
          key={key}
          className={cn(
            "flex h-6 min-w-6 items-center justify-center rounded-sm px-1.5 font-sans text-xs font-medium",
            tone === "live" && "bg-black/10",
            tone === "stage" && "bg-white/15",
            tone === "plain" && "border border-border bg-muted text-foreground",
          )}
        >
          {key.trim()}
        </kbd>
      ))}
    </span>
  );
}

/** The on-air stage: the one place Home spends colour. */
function DictationStage() {
  const { recording, busy, toggle } = useRecording();
  const partial = useStore((s) => s.partial);
  const settings = useStore((s) => s.settings);
  const catalog = useStore((s) => s.catalog);
  const recordingMode = useStore((s) => s.recordingMode);
  const live = recording && recordingMode !== "test";
  const bars = useLevelHistory(recording);
  const activeModel = catalog.find((m) => m.id === settings?.stt_model);
  const needsModel = catalog.length > 0 && !activeModel?.installed;
  const hotkey = formatHotkey(settings?.hotkey ?? DEFAULT_HOTKEY);

  // After a dictation lands, the stage says what arrived for a moment while the
  // waveform of what was just said settles back to rest.
  const lastResult = useStore((s) => s.lastResult);
  const [landed, setLanded] = useState<api.TranscriptResult | null>(null);
  const seenResult = useRef(lastResult);
  useEffect(() => {
    if (lastResult === seenResult.current) return;
    seenResult.current = lastResult;
    if (!lastResult || lastResult.error || !lastResult.text.trim()) return;
    setLanded(lastResult);
    const timer = window.setTimeout(() => setLanded(null), 4500);
    return () => window.clearTimeout(timer);
  }, [lastResult]);
  const showLanded = landed !== null && !recording;
  const landedWords = landed ? wordCount(landed.text) : 0;

  return (
    <section
      aria-label="Dictation"
      className={cn(
        "flex flex-col gap-4 rounded-xl p-5 transition-colors duration-300",
        live ? "bg-record-live text-record-live-foreground" : "bg-stage text-stage-foreground",
      )}
    >
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex items-center gap-2">
          {showLanded ? (
            <Check key={landed.text} className="animate-od-pop size-4" strokeWidth={2.5} aria-hidden />
          ) : (
            <span
              className={cn(
                "size-2.5 rounded-full",
                live ? "animate-od-blink bg-record-live-foreground" : "bg-[#EFECF0]",
              )}
              aria-hidden
            />
          )}
          <h2 className="text-base font-semibold">
            {live
              ? "On air"
              : showLanded
                ? `${landed.copied ? "Copied" : landed.injected ? "Inserted" : "Transcribed"} · ${landedWords} ${landedWords === 1 ? "word" : "words"}`
                : needsModel
                  ? "One step left"
                  : "Ready to dictate"}
          </h2>
        </div>
        <Keycaps hotkey={hotkey} tone={live ? "live" : "stage"} />
      </div>

      <div className="flex h-14 items-center gap-[3px]" aria-hidden>
        {bars.map((value, index) => (
          <span
            key={index}
            className={cn(
              "min-h-1 flex-1 rounded-full",
              live ? "bg-record-live-foreground" : "bg-white/35",
              !recording && "motion-safe:transition-[height] motion-safe:duration-700 motion-safe:ease-(--ease-fluid)",
            )}
            style={{ height: `${Math.max(6, value * 100)}%` }}
          />
        ))}
      </div>

      <p className={cn("min-h-5 truncate text-sm", live || showLanded ? "" : "text-stage-muted")} aria-live="polite">
        {showLanded
          ? `“${tailForDisplay(landed.text, 110)}”`
          : partial
          ? `“${tailForDisplay(partial, 110)}”`
          : live
            ? "Listening. Your words appear in the app you are using."
            : needsModel
              ? "Download a speech model below to start."
              : "Speak in any app. Nothing leaves this device."}
      </p>

      <div>
        <button
          type="button"
          onClick={toggle}
          disabled={busy}
          className={cn(
            "inline-flex h-11 cursor-pointer items-center gap-2 rounded-full px-5 text-sm font-semibold transition-colors duration-150 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-current disabled:cursor-not-allowed disabled:opacity-60",
            recording
              ? "bg-record-live-foreground text-white hover:bg-record-live-foreground/90"
              : "bg-record text-record-foreground hover:bg-record-hover",
          )}
        >
          {recording ? (
            <Square className="size-3.5 fill-current" aria-hidden />
          ) : (
            <Mic className="size-4" aria-hidden />
          )}
          {recording ? "Stop" : "Record"}
        </button>
      </div>
    </section>
  );
}

const MILESTONE_KEY = "od:milestone-words";

function readMilestone(): number | null {
  try {
    const value = localStorage.getItem(MILESTONE_KEY);
    return value === null ? null : Number(value);
  } catch {
    return null;
  }
}

/** Marks a word milestone once, the next time Home is open after crossing it. */
function MilestoneNote() {
  const total = useStore((s) => s.stats?.total_words);
  const [mark, setMark] = useState<number | null>(null);
  useEffect(() => {
    if (total === undefined) return;
    const { show, store } = nextMilestone(total, readMilestone());
    try {
      localStorage.setItem(MILESTONE_KEY, String(store));
    } catch {
      // Without storage the note may repeat; it is still correct.
    }
    if (show !== null) setMark(show);
  }, [total]);

  if (mark === null) return null;
  const { title, body } = milestoneCopy(mark);
  return (
    <section
      role="status"
      aria-label="Milestone"
      className="animate-od-pop flex items-center gap-3 rounded-lg bg-positive-soft px-4 py-3 text-positive"
    >
      <Sparkles className="size-4 shrink-0" strokeWidth={1.75} aria-hidden />
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <p className="text-sm font-semibold">{title}</p>
        <p className="text-xs text-positive/80">{body}</p>
      </div>
      <Button size="sm" variant="ghost" className="text-positive hover:text-positive" onClick={() => setMark(null)}>
        Dismiss
      </Button>
    </section>
  );
}

function LastResultPanel() {
  const lastResult = useStore((s) => s.lastResult);
  const settings = useStore((s) => s.settings);
  const hotkey = formatHotkey(settings?.hotkey ?? DEFAULT_HOTKEY);
  const saved = lastResult && !lastResult.error ? secondsSaved(lastResult.text, lastResult.duration_ms) : null;
  const undoHint = settings?.voice_actions_enabled
    ? "Undo returns to the previous application, or say “scratch that”."
    : "Undo returns to the previous application.";
  const [undoneResult, setUndoneResult] = useState<api.TranscriptResult | null>(null);
  const [undoing, setUndoing] = useState(false);
  const undone = lastResult !== null && undoneResult === lastResult;
  const inserted = lastResult?.injected === true && !lastResult.copied && !lastResult.error;

  return (
    <div className="flex flex-col gap-2">
      <SectionTitle>Last result</SectionTitle>
      {!lastResult ? (
        <div className="rounded-lg border border-dashed border-input px-4 py-3 text-sm leading-7 text-muted-foreground">
          Nothing inserted yet. Press <Keycaps hotkey={hotkey} tone="plain" /> in any app and speak.
        </div>
      ) : (
        <Card>
          <CardContent className="flex items-center gap-3 p-4">
            <StatusChip ok={!lastResult.error}>
              {lastResult.error ? "Not inserted" : inserted ? "Inserted" : lastResult.copied ? "Copied" : "Transcribed"}
            </StatusChip>
            <span className="min-w-0 flex-1 truncate text-sm font-medium">
              “{lastResult.text}”
            </span>
            {/* Only the first item of the trailing group takes ml-auto. A
                second one splits the free space between them instead of
                pushing the group flush right, which left Undo stranded
                mid-row with a gap before the duration. */}
            <Button
              size="sm"
              variant="outline"
              className="ml-auto shrink-0"
              disabled={!inserted || undone || undoing}
              title={inserted ? "Undo in the previous application" : "Undo is available only for inserted text"}
              onClick={async () => {
                if (!inserted || undoing || useStore.getState().lastResult !== lastResult) return;
                const resultToUndo = lastResult;
                setUndoing(true);
                try {
                  await api.undoLastInsert();
                  setUndoneResult(resultToUndo);
                } catch (e) {
                  toast.error(String(e));
                } finally {
                  setUndoing(false);
                }
              }}
            >
              <Undo2 className="size-3.5" aria-hidden />
              {undone ? "Undone" : "Undo"}
            </Button>
            {lastResult.duration_ms > 0 && (
              <span className="shrink-0 text-xs text-muted-foreground tabular-nums">
                {(lastResult.duration_ms / 1000).toFixed(1)}s
              </span>
            )}
          </CardContent>
        </Card>
      )}
      {lastResult && (
        <p className="text-xs text-pretty text-muted-foreground">
          {lastResult.error ? `Insertion failed: ${lastResult.error}`
            : inserted ? `${saved ? `About ${formatSpan(saved)} quicker than typing it. ` : ""}${undoHint}`
            : lastResult.copied ? "Copied to clipboard. Paste into your application."
            : "Text is available here; insertion was not confirmed."}
        </p>
      )}
    </div>
  );
}

export function HomeTab() {
  return (
    <div className="flex flex-col gap-5">
      <DictationStage />
      <MilestoneNote />
      <LastResultPanel />
      <ReadyStrip />
    </div>
  );
}
