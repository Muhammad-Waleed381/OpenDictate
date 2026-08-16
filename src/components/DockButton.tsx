import { useEffect, useRef, useState, type ReactNode } from "react";
import { useStore } from "@/lib/store";
import * as api from "@/lib/api";

const BARS = 8;
const BAR_WIDTH = 2;
const BAR_GAP = 1;
const BUFFER_SIZE = 32;

function Waveform({ active, color }: { active: boolean; color: string }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const bufferRef = useRef<number[]>([]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    let raf = 0;
    const draw = () => {
      const level = active ? useStore.getState().level : 0.3;
      const buffer = bufferRef.current;
      buffer.push(level);
      if (buffer.length > BUFFER_SIZE) buffer.shift();

      const w = canvas.width;
      const h = canvas.height;
      ctx.clearRect(0, 0, w, h);
      ctx.fillStyle = color;

      const half = Math.floor(BARS / 2);
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
  }, [active, color]);

  return (
    <canvas
      ref={canvasRef}
      width={BARS * BAR_WIDTH + (BARS - 1) * BAR_GAP}
      height={14}
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
  const canStop = active && state !== "transcribing";

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
      <span className="animate-od-pop text-sm font-bold text-green-400">✓</span>
    );
  } else if (flash === "error" || error) {
    content = (
      <span className="animate-od-pop text-[10px] font-bold text-red-400">
        ✕
      </span>
    );
  } else if (state === "transcribing") {
    content = (
      <span className="flex gap-0.5">
        {[0, 1, 2].map((i) => (
          <span
            key={i}
            className="size-1 animate-od-bounce-y bg-white"
            style={{ animationDelay: `${i * 0.15}s` }}
          />
        ))}
      </span>
    );
  } else {
    content = (
      <Waveform active={active} color={active ? "#ffffff" : "#000000"} />
    );
  }

  return (
    <button
      type="button"
      onClick={toggle}
      className={`fixed inset-0 cursor-pointer rounded-full border-2 transition-transform hover:scale-110 ${flash === "error" || error ? "border-red-400 bg-black" : flash === "inserted" ? "border-green-400 bg-black" : active || state === "transcribing" ? "border-black bg-black" : "border-black bg-white opacity-80"}`}
      aria-label={canStop ? "Stop recording" : "Start recording"}
      title={canStop ? "Stop recording (Ctrl+K)" : "Start recording (Ctrl+K)"}
    >
      {content}
    </button>
  );
}