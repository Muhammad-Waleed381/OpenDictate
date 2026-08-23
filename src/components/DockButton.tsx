import { useEffect, useRef, useState, type ReactNode } from "react";
import { Check, Mic, X } from "lucide-react";
import { useStore } from "@/lib/store";
import * as api from "@/lib/api";
import { tailForDisplay } from "@/lib/utils";

export function DockButton() {
  const overlayState = useStore((s) => s.overlayState);
  const recording = useStore((s) => s.recording);
  const partial = useStore((s) => s.partial);
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
    } else if (!active) {
      try {
        await api.startRecording("dictate");
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

  const listening = active;
  const processing = state === "transcribing";
  const rawLabel =
    partial === "listening…" ? "RECORDING"
    : partial === "transcribing…" ? "PROCESSING"
    : partial;
  const pillLabel = tailForDisplay(rawLabel, 46);

  return (
    <div className="flex h-full w-full items-end justify-between gap-2 pr-2 pl-3">
      {pillLabel && (
        <span
          className={`flex min-w-0 items-center gap-2 rounded-full px-3 py-1.5 text-[11px] leading-none font-bold tracking-wider text-white shadow-lg ring-1 ${
            listening
              ? "bg-black/90 ring-red-400/70"
              : processing
                ? "bg-black/90 ring-amber-400/70"
                : "bg-black/85 ring-white/10"
          }`}
        >
          {(listening || processing) && (
            <span
              className={`size-2 shrink-0 animate-pulse rounded-full ${
                listening ? "bg-red-500" : "bg-amber-400"
              }`}
            />
          )}
          <span className="min-w-0 truncate">{pillLabel}</span>
          {(listening || processing) && (
            <span className="flex h-3 shrink-0 items-end gap-[1.5px]">
              {[0, 1, 2].map((i) => (
                <span
                  key={i}
                  className={`w-[2px] animate-od-eq origin-bottom ${
                    listening ? "bg-red-400" : "bg-amber-400"
                  }`}
                  style={{
                    height: 10,
                    animationDelay: `${i * 0.15}s`,
                    animationDuration: "0.7s",
                  }}
                />
              ))}
            </span>
          )}
        </span>
      )}
      <div className="relative flex size-6 shrink-0 items-center justify-center">
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
          className={`relative z-10 flex size-6 cursor-pointer items-center justify-center rounded-full shadow-lg ring-1 ${live ? "bg-black/90" : "bg-white/90"} ${tint}`}
          aria-label={canStop ? "Stop recording" : "Start recording"}
          title={canStop ? "Stop recording (Ctrl+K)" : "Start recording (Ctrl+K)"}
        >
          {content}
        </button>
      </div>
    </div>
  );
}
