import { useEffect, useRef, useState } from "react";
import { useStore } from "@/lib/store";
import * as api from "@/lib/api";
import { Button } from "@/components/ui/button";

export function MicTest() {
  const [testing, setTesting] = useState(false);
  const [noAudioHint, setNoAudioHint] = useState(false);
  const [peak, setPeak] = useState(0);
  const [verdict, setVerdict] = useState<"working" | "quiet" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const peakRef = useRef(0);

  const handleStart = async () => {
    peakRef.current = 0;
    setPeak(0);
    setVerdict(null);
    setNoAudioHint(false);
    setError(null);
    setTesting(true);
    try {
      await api.cancelRecording().catch(() => {});
      await api.startRecording("test");
    } catch (e) {
      setTesting(false);
      setError(String(e));
    }
  };

  const handleStop = async () => {
    try {
      await api.stopRecording();
    } catch (e) {
      setError(String(e));
    } finally {
      setTesting(false);
      setVerdict(peakRef.current > 0.05 ? "working" : "quiet");
      setPeak(peakRef.current);
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
      if (testingRef.current) {
        api.cancelRecording().catch(() => {});
      }
    };
  }, []);

  const level = useStore((s) => s.level);

  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-center justify-between">
        <span className="text-sm font-bold uppercase tracking-wider">
          {testing ? "Speak into the mic…" : "Test your microphone"}
        </span>
        <Button
          onClick={testing ? handleStop : handleStart}
          variant={testing ? "outline" : "default"}
          size="sm"
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