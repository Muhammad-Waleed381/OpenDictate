import { useEffect, useRef, useState, type ReactNode } from "react";
import { Check, Mic, X } from "lucide-react";
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

  const live = active || state === "transcribing";
  const tint = flash === "inserted" || state === "inserted" ? "ring-green-400"
    : flash === "error" || error ? "ring-red-400"
    : state === "transcribing" ? "ring-amber-400"
    : active ? "ring-red-400"
    : "ring-black/20";

  let content: ReactNode;
  if (flash === "inserted") {
    content = <Check className="size-[16px] text-green-400" strokeWidth={3} />;
  } else if (flash === "error" || error) {
    content = <X className="size-[16px] text-red-400" strokeWidth={3} />;
  } else if (state === "transcribing") {
    content = (
      <span className="flex items-end gap-[1.5px]">
        {[0, 1, 2].map((i) => (
          <span
            key={i}
            className="h-3 w-[2px] animate-od-eq origin-bottom bg-white"
            style={{
              animationDelay: `${i * 0.15}s`,
              animationDuration: "0.8s",
            }}
          />
        ))}
      </span>
    );
  } else if (active) {
    content = (
      <span className="flex h-[14px] items-end gap-[1.5px]">
        {[0, 1, 2, 3].map((i) => (
          <span
            key={i}
            className="w-[2px] animate-od-eq origin-bottom bg-white"
            style={{
              height: 12,
              animationDelay: `${i * 0.18}s`,
              animationDuration: `${0.75 + (i % 2) * 0.2}s`,
            }}
          />
        ))}
      </span>
    );
  } else {
    content = <Mic className="size-[17px] text-slate-900" strokeWidth={2.5} />;
  }

  return (
    <div className="relative flex size-6 items-center justify-center">
      {(active || state === "transcribing") && (
        <span
          key={`${state}-${recording}`}
          className={`pointer-events-none absolute inset-0 animate-od-ping rounded-full ${
            state === "transcribing" ? "bg-amber-400/40" : "bg-red-400/50"
          }`}
        />
      )}
      <button
        type="button"
        onClick={toggle}
        className={`relative z-10 flex size-6 cursor-pointer items-center justify-center rounded-full shadow-lg ring-1 transition-transform hover:scale-110 ${live ? "bg-black/90" : "bg-white/90"} ${tint}`}
        aria-label={canStop ? "Stop recording" : "Start recording"}
        title={canStop ? "Stop recording (Ctrl+K)" : "Start recording (Ctrl+K)"}
      >
        {content}
      </button>
    </div>
  );
}
