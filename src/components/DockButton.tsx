import { useEffect, useRef, useState, type ReactNode } from "react";
import { useStore } from "@/lib/store";
import * as api from "@/lib/api";

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
    content = <span className="animate-od-pop text-[14px] font-bold text-green-400">✓</span>;
  } else if (flash === "error" || error) {
    content = <span className="animate-od-pop text-[14px] font-bold text-red-400">✕</span>;
  } else if (state === "transcribing") {
    content = (
      <span className="flex gap-[2px]">
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
      <span
        className={`block ${active ? "animate-od-blink bg-white" : "bg-black"}`}
        style={{
          width: 6,
          height: active ? 10 : 8,
        }}
      />
    );
  }

  return (
    <button
      type="button"
      onClick={toggle}
      className={`absolute right-0 top-0 flex size-6 cursor-pointer items-center justify-center rounded-full border transition-transform hover:scale-150 ${flash === "error" || error ? "border-red-400 bg-black" : flash === "inserted" ? "border-green-400 bg-black" : active || state === "transcribing" ? "border-black bg-black" : "border-black bg-white opacity-80"}`}
      aria-label={canStop ? "Stop recording" : "Start recording"}
      title={canStop ? "Stop recording (Ctrl+K)" : "Start recording (Ctrl+K)"}
    >
      {content}
    </button>
  );
}