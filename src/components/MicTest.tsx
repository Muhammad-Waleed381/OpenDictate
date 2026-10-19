import { useEffect, useRef, useState } from "react";
import { useStore } from "@/lib/store";
import * as api from "@/lib/api";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { toast } from "@/components/ui/toast";

const micLabel = (mic: string | null, mics: api.MicDevice[]): string => {
  if (mic === null) return "Select microphone";
  const found = mics.find((m) => m.id === mic);
  return found?.label ?? mic;
};

export function MicTest() {
  const mics = useStore((s) => s.mics);
  const mic = useStore((s) => s.mic);
  const recording = useStore((s) => s.recording);
  const busy = useStore((s) => s.recordingBusy);
  const recordingMode = useStore((s) => s.recordingMode);
  const [testing, setTesting] = useState(false);
  const [noAudioHint, setNoAudioHint] = useState(false);
  const [peak, setPeak] = useState(0);
  const [verdict, setVerdict] = useState<"working" | "quiet" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const peakRef = useRef(0);

  useEffect(() => {
    if (recordingMode !== "test" && !busy) setTesting(false);
  }, [recordingMode, busy]);

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

  const handleStart = async () => {
    if (useStore.getState().recording || useStore.getState().recordingBusy) return;
    peakRef.current = 0;
    setPeak(0);
    setVerdict(null);
    setNoAudioHint(false);
    setError(null);
    setTesting(true);
    testingRef.current = true;
    try {
      if (!await useStore.getState().startRecording("test")) setTesting(false);
    } catch (e) {
      setTesting(false);
      setError(String(e));
    }
  };

  const handleStop = async () => {
    if (useStore.getState().recordingBusy) return;
    if (useStore.getState().recordingMode !== "test") { setTesting(false); return; }
    try {
      const result = await useStore.getState().stopRecording();
      if (!result) return;
      const quiet = peakRef.current <= 0.05;
      setTesting(false);
      setVerdict(quiet ? "quiet" : "working");
      setPeak(peakRef.current);
      // No audio at all: surface the platform-specific hint (macOS mic
      // permission is the most common cause and was previously unreachable).
      setNoAudioHint(quiet);
    } catch (e) {
      setError(String(e));
      setTesting(useStore.getState().recordingMode === "test");
    }
  };

  useEffect(() => {
    if (!testing) return;
    const id = setInterval(() => {
      const level = useStore.getState().level;
      if (level > peakRef.current) {
        peakRef.current = level;
        setPeak(level);
      }
    }, 50);
    return () => clearInterval(id);
  }, [testing]);

  const testingRef = useRef(false);
  testingRef.current = testing;

  useEffect(() => {
    return () => {
      if (testingRef.current && useStore.getState().recordingMode === "test") {
        useStore.getState().cancelRecording().catch(() => {});
      }
    };
  }, []);

  const level = useStore((s) => s.level);

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-col gap-1.5">
        <label className="text-xs font-bold text-muted-foreground uppercase">Microphone Device</label>
        <Select value={mic ?? ""} onValueChange={handleMicChange} disabled={recording || busy}>
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
      </div>

      <div className="flex items-center justify-between">
        <span className="text-sm font-bold uppercase tracking-wider">
          {testing ? "Speak into the mic…" : "Test your microphone"}
        </span>
        <Button
          onClick={testing ? handleStop : handleStart}
          variant={testing ? "outline" : "default"}
          size="sm"
          disabled={busy || (!testing && recording)}
        >
          {testing ? "■ Stop" : "● Start"}
        </Button>
      </div>
      <div className="flex h-8 items-end gap-[3px] border-2 border-border bg-card p-1.5">
        {Array.from({ length: 24 }, (_, i) => {
          const sample = i / 23;
          const lit = level > sample;
          return (
            <span
              key={i}
              className={`flex-1 transition-colors duration-75 ${lit ? "bg-primary" : "bg-muted"}`}
              style={{ height: "100%" }}
            />
          );
        })}
      </div>
      <div className="flex items-center justify-between text-xs font-bold uppercase tracking-wider">
        <span className="tabular-nums">Peak {(peak * 100).toFixed(0)}%</span>
        {verdict === "working" && (
          <span className="flex items-center gap-1.5 border-2 border-primary bg-primary px-2 py-0.5 text-primary-foreground">
            ✓ Mic working
          </span>
        )}
        {verdict === "quiet" && (
          <span className="flex animate-od-blink items-center gap-1.5 border-2 border-border bg-card px-2 py-0.5">
            ✕ Too quiet
          </span>
        )}
        {testing && <span className="animate-od-blink text-muted-foreground">Listening…</span>}
      </div>
      {error && (
        <div className="border-2 border-primary bg-primary px-2 py-1.5 text-xs font-bold text-primary-foreground uppercase">
          ✕ {error}
        </div>
      )}
      {noAudioHint && (
        <p className="border-2 border-dashed border-border px-2 py-1.5 text-xs text-muted-foreground">
          No audio received. On macOS allow the microphone under System Settings →
          Privacy &amp; Security → Microphone, then press Stop and Start again. Also
          check the selected input device in Settings → Microphone.
        </p>
      )}
    </div>
  );
}
