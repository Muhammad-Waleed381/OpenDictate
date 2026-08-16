import { useEffect, useRef, useState, type ReactNode } from "react";
import { useStore } from "@/lib/store";
import * as api from "@/lib/api";

const BAR_COUNT = 18;
const BAR_WIDTH = 3;
const BAR_GAP = 2;
const BUFFER_SIZE = 32;

function Waveform({ active }: { active: boolean }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const bufferRef = useRef<number[]>([]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    let raf = 0;
    const draw = () => {
      const level = active ? useStore.getState().level : 0.22;
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
  }, [active]);

  return (
    <canvas
      ref={canvasRef}
      width={BAR_COUNT * BAR_WIDTH + (BAR_COUNT - 1) * BAR_GAP}
      height={26}
    />
  );
}

export function DockButton() {
  const overlayState = useStore((s) => s.overlayState);
  const recording = useStore((s) => s.recording);
  const [error, setError] = useState<string | null>(null);
  const [flash, setFlash] = useState<"inserted" | "error" | null>(null);
  const flashTimer = useRef<number | null>(null);

  const state = overlayState?.state ?? "hidden";
  const active = state === "listening" || recording;
  const canStop = state === "listening";

  useEffect(() => {
    if (state === "inserted") {
      setFlash("inserted");
      flashTimer.current = window.setTimeout(() => {
        setFlash(null);
        setError(null);
      }, 1400);
    } else if (state === "error") {
      setFlash("error");
      flashTimer.current = window.setTimeout(() => {
        setFlash(null);
        setError(null);
      }, 2400);
    }
    return () => {
      if (flashTimer.current) window.clearTimeout(flashTimer.current);
    };
  }, [state]);

  const toggle = async () => {
    setError(null);
    if (canStop) {
      try {
        const result = await api.stopRecording();
        if (result?.text) {
          useStore.setState({ lastResult: result });
        }
      } catch (e) {
        setError(String(e));
        setFlash("error");
        flashTimer.current = window.setTimeout(() => {
          setFlash(null);
          setError(null);
        }, 2400);
      }
      useStore.getState().setRecording(false);
    } else if (!active) {
      try {
        await api.startRecording("dictate");
        useStore.getState().setRecording(true);
      } catch (e) {
        setError(String(e));
        setFlash("error");
        flashTimer.current = window.setTimeout(() => {
          setFlash(null);
          setError(null);
        }, 2400);
      }
    }
  };

  let content: ReactNode;
  if (flash === "inserted") {
    content = (
      <div className="flex h-10 w-[140px] animate-od-pop items-center justify-center gap-2 border-2 border-black bg-black text-white">
        <span className="text-[11px] font-bold tracking-[0.2em] uppercase">
          Inserted
        </span>
        <span className="text-sm font-bold">✓</span>
      </div>
    );
  } else if (flash === "error" || error) {
    content = (
      <div className="flex h-10 w-[140px] animate-od-pop items-center justify-center gap-2 border-2 border-black bg-black px-2 text-white">
        <span className="truncate text-[10px] font-bold tracking-wider uppercase">
          ✕ {error ?? "Error"}
        </span>
      </div>
    );
  } else if (state === "transcribing") {
    content = (
      <div className="flex h-10 w-[140px] items-center justify-center gap-2 border-2 border-black bg-white">
        <span className="flex gap-1">
          {[0, 1, 2].map((i) => (
            <span
              key={i}
              className="size-1.5 animate-od-bounce-y bg-black"
              style={{ animationDelay: `${i * 0.15}s` }}
            />
          ))}
        </span>
        <span className="text-[11px] font-bold tracking-[0.2em] uppercase">
          Transcribing
        </span>
      </div>
    );
  } else {
    content = (
      <div
        className={`flex h-10 w-[140px] cursor-pointer items-center justify-center gap-2.5 border-2 border-black bg-white px-3 transition-transform hover:scale-[1.03] ${active ? "" : "opacity-85"}`}
      >
        {active && (
          <span className="size-2 animate-od-blink border-2 border-black bg-black" />
        )}
        <Waveform active={active} />
      </div>
    );
  }

  return (
    <button
      type="button"
      onClick={toggle}
      className="fixed inset-0 cursor-pointer"
      aria-label={canStop ? "Stop recording" : "Start recording"}
      title={canStop ? "Stop recording" : "Start recording"}
    >
      {content}
    </button>
  );
}