import { useEffect, useRef } from "react";
import { useStore } from "@/lib/store";

const BAR_COUNT = 24;
const BAR_WIDTH = 4;
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
      ctx.fillStyle = "#000000";

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

  return (
    <div className="pointer-events-none fixed inset-0 flex items-start justify-center pt-5">
      {state === "listening" && (
        <div className="animate-od-slide-up pointer-events-none flex h-[64px] max-w-[360px] items-center justify-center gap-4 border-2 border-black bg-white px-7 shadow-[6px_6px_0_0_#000]">
          <span className="size-3 border-2 border-black bg-black animate-od-blink" />
          <span className="text-sm font-bold tracking-[0.25em] uppercase">
            Listening
          </span>
          <Waveform />
        </div>
      )}
      {state === "transcribing" && (
        <div className="animate-od-slide-up pointer-events-none relative flex h-[64px] max-w-[360px] items-center justify-center gap-3 overflow-hidden border-2 border-black bg-black px-7 shadow-[6px_6px_0_0_#000]">
          <span className="animate-od-shimmer absolute inset-0" />
          <span className="relative text-sm font-bold tracking-[0.25em] text-white uppercase">
            Transcribing
          </span>
          <span className="relative flex gap-1">
            {[0, 1, 2].map((i) => (
              <span
                key={i}
                className="size-1.5 animate-od-bounce-y bg-white"
                style={{ animationDelay: `${i * 0.15}s` }}
              />
            ))}
          </span>
        </div>
      )}
      {state === "inserted" && (
        <div className="animate-od-pop pointer-events-none flex h-[64px] max-w-[360px] items-center justify-center gap-3 border-2 border-black bg-black px-7 shadow-[6px_6px_0_0_#000]">
          <span className="text-sm font-bold tracking-[0.25em] text-white uppercase">
            Inserted
          </span>
          <span className="text-base font-bold text-white">✓</span>
        </div>
      )}
      {state === "error" && (
        <div className="animate-od-slide-up pointer-events-none flex h-[64px] max-w-[360px] items-center justify-center gap-3 border-2 border-black bg-white px-7 shadow-[6px_6px_0_0_#000]">
          <span className="size-3 border-2 border-black bg-black animate-od-blink" />
          <span className="text-sm font-bold tracking-[0.25em] uppercase">
            {message ?? "Error"}
          </span>
        </div>
      )}
    </div>
  );
}