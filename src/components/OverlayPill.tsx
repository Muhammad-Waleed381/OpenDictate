import { useEffect, useRef } from "react";
import { useStore } from "@/lib/store";

const BAR_COUNT = 24;
const BAR_WIDTH = 3;
const BAR_GAP = 2;
const BUFFER_SIZE = 32;

function Waveform() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const bufferRef = useRef<number[]>([]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    let raf = 0;
    const draw = () => {
      const level = useStore.getState().level;
      const buffer = bufferRef.current;
      buffer.push(level);
      if (buffer.length > BUFFER_SIZE) buffer.shift();

      const w = canvas.width;
      const h = canvas.height;
      ctx.clearRect(0, 0, w, h);
      ctx.fillStyle = "#3B82F6";

      const half = Math.floor(BAR_COUNT / 2);
      for (let i = 0; i < half; i++) {
        const sample =
          buffer[Math.floor((i / half) * Math.max(buffer.length - 1, 0))] ?? 0;
        const barHeight = Math.max(2, Math.min(h, sample * h));
        const xLeft = (half - 1 - i) * (BAR_WIDTH + BAR_GAP);
        const xRight = w - (half - i) * (BAR_WIDTH + BAR_GAP);
        const yMid = (h - barHeight) / 2;
        ctx.fillRect(xLeft, yMid, BAR_WIDTH, barHeight);
        ctx.fillRect(xRight, yMid, BAR_WIDTH, barHeight);
      }
      raf = requestAnimationFrame(draw);
    };
    raf = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(raf);
  }, []);

  return (
    <canvas
      ref={canvasRef}
      width={BAR_COUNT * BAR_WIDTH + (BAR_COUNT - 1) * BAR_GAP}
      height={32}
      className="h-8"
    />
  );
}

export function OverlayPill() {
  const overlayState = useStore((s) => s.overlayState);

  if (!overlayState || overlayState.state === "hidden") return null;

  const { state, message } = overlayState;

  const styles: Record<string, string> = {
    listening: "border-[#3B82F6]/70 text-[#F8FAFC] shadow-[0_0_24px_rgba(59,130,246,0.45)]",
    transcribing: "border-[#64748B]/60 text-[#F8FAFC]",
    inserted: "border-[#10B981]/70 text-[#10B981] shadow-[0_0_24px_rgba(16,185,129,0.4)]",
    error: "border-[#EF4444]/70 text-[#EF4444] shadow-[0_0_24px_rgba(239,68,68,0.4)]",
  };

  return (
    <div className="pointer-events-none fixed inset-0 flex items-start justify-center pt-5">
      <div
        className={`pointer-events-none flex h-[76px] max-w-[360px] items-center justify-center gap-3 rounded-full border bg-[#0F172A]/85 px-6 backdrop-blur-md ${styles[state] ?? ""}`}
      >
        {state === "listening" && (
          <>
            <span className="text-sm font-medium">Listening…</span>
            <Waveform />
          </>
        )}
        {state === "transcribing" && (
          <span className="animate-pulse text-sm font-medium">
            Transcribing…
          </span>
        )}
        {state === "inserted" && (
          <span className="text-sm font-medium">Inserted</span>
        )}
        {state === "error" && (
          <span className="text-sm font-medium">{message ?? "Error"}</span>
        )}
      </div>
    </div>
  );
}