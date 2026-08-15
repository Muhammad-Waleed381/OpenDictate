import { useEffect, useRef, useState } from "react";
import { useStore } from "@/lib/store";
import * as api from "@/lib/api";
import { Button } from "@/components/ui/button";

export function MicTest() {
  const [testing, setTesting] = useState(false);
  const [peak, setPeak] = useState(0);
  const [verdict, setVerdict] = useState<"working" | "quiet" | null>(null);
  const peakRef = useRef(0);

  const handleStart = async () => {
    peakRef.current = 0;
    setPeak(0);
    setVerdict(null);
    setTesting(true);
    try {
      await api.startRecording("test");
    } catch {
      setTesting(false);
    }
  };

  const handleStop = async () => {
    try {
      await api.stopRecording();
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
        <span className="text-sm text-[#64748B]">
          {testing ? "Speak into the microphone…" : "Test your microphone"}
        </span>
        <Button
          onClick={testing ? handleStop : handleStart}
          variant={testing ? "destructive" : "default"}
          size="sm"
        >
          {testing ? "Stop test" : "Start test"}
        </Button>
      </div>
      <div className="h-2 w-full overflow-hidden rounded-full bg-[#334155]">
        <div
          className="h-full rounded-full bg-[#3B82F6] transition-[width] duration-75"
          style={{ width: `${Math.min(100, Math.max(0, level * 100))}%` }}
        />
      </div>
      <div className="flex items-center justify-between text-xs text-[#64748B]">
        <span>Peak: {(peak * 100).toFixed(0)}%</span>
        {verdict === "working" && (
          <span className="font-medium text-[#10B981]">Mic working</span>
        )}
        {verdict === "quiet" && (
          <span className="font-medium text-[#EF4444]">Too quiet</span>
        )}
      </div>
    </div>
  );
}