import { useEffect, useRef, useState, type ReactNode } from "react";
import { Check, Mic, X } from "lucide-react";
import { useStore } from "@/lib/store";
import { tailForDisplay } from "@/lib/utils";

export function DockButton() {
  const overlayState = useStore((s) => s.overlayState);
  const recording = useStore((s) => s.recording);
  const busy = useStore((s) => s.recordingBusy || s.recordingMode === "test");
  const partial = useStore((s) => s.partial);
  const [error, setError] = useState<string | null>(null);
  const [flash, setFlash] = useState<"inserted" | "error" | null>(null);
  const flashTimer = useRef<number | null>(null);

  const state = overlayState?.state ?? "hidden";

  const scheduleFlashClear = (ms: number) => {
    if (flashTimer.current) window.clearTimeout(flashTimer.current);
    flashTimer.current = window.setTimeout(() => {
      setFlash(null);
      setError(null);
    }, ms);
  };

  useEffect(() => {
    if (state === "inserted") {
      setFlash("inserted");
      scheduleFlashClear(1400);
    } else if (state === "error") {
      setFlash("error");
      scheduleFlashClear(2400);
    } else {
      setFlash(null);
      setError(null);
    }
    return () => {
      if (flashTimer.current) window.clearTimeout(flashTimer.current);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [state]);

  const toggle = async () => {
    const store = useStore.getState();
    if (store.recordingBusy || store.recordingMode === "test" || isProcessing) return;
    try {
      setError(null);
      if (store.recording) await store.stopRecording();
      // "listening" without a recording is awake Handsfree, which a click can
      // take over with an explicit recording.
      else if (!active || isListening) await store.startRecording("dictate");
    } catch (e) {
      setError(String(e));
      setFlash("error");
      scheduleFlashClear(2400);
    }
  };

  const isRecording = state === "recording" || (recording && state !== "transcribing");
  const isListening = state === "listening" && !recording;
  const isProcessing = state === "transcribing";
  const isInserted = flash === "inserted";
  const isError = flash === "error" || error != null;

  const active = isRecording || isListening;
  const canStop = (isRecording || recording) && !isProcessing;

  const live = active || isProcessing;
  const tint = isInserted ? "ring-[#A594FF]"
    : isError ? "ring-[#F354D0]"
    : isProcessing ? "ring-[#EFECF0]"
    : isRecording ? "ring-[#F354D0]"
    : isListening ? "ring-[#A594FF]"
    : "ring-black/20";

  let content: ReactNode;
  if (isInserted) {
    content = <Check className="size-[16px] text-[#A594FF]" strokeWidth={3} />;
  } else if (isError) {
    content = <X className="size-[16px] text-[#F354D0]" strokeWidth={3} />;
  } else if (isProcessing) {
    content = (
      <span className="flex items-end gap-[1.5px]">
        {[0, 1, 2].map((i) => (
          <span
            key={i}
            className="h-3 w-[2px] animate-od-eq origin-bottom bg-[#EFECF0]"
            style={{
              animationDelay: `${i * 0.15}s`,
              animationDuration: "0.8s",
            }}
          />
        ))}
      </span>
    );
  } else if (isRecording) {
    content = (
      <span className="flex h-[14px] items-end gap-[1.5px]">
        {[0, 1, 2, 3].map((i) => (
          <span
            key={i}
            className="w-[2px] animate-od-eq origin-bottom bg-[#F354D0]"
            style={{
              height: 12,
              animationDelay: `${i * 0.18}s`,
              animationDuration: `${0.75 + (i % 2) * 0.2}s`,
            }}
          />
        ))}
      </span>
    );
  } else if (isListening) {
    content = <Mic className="size-[17px] text-[#A594FF]" strokeWidth={2.5} />;
  } else {
    content = <Mic className="size-[17px] text-[#1A1240]" strokeWidth={2.5} />;
  }

  const rawLabel =
    partial === "listening…" || partial === "Listening…" ? "LISTENING"
    : partial === "recording…" || partial === "Recording…" ? "RECORDING"
    : partial === "transcribing…" || partial === "Processing…" ? "PROCESSING"
    : partial;
  const pillLabel = tailForDisplay(rawLabel, 46);

  const settings = useStore((s) => s.settings);
  const dockPosition = settings?.dock_position ?? "bottom_right";
  const isRightAligned = dockPosition === "bottom_right" || dockPosition === "top_right";
  const isLeftAligned = dockPosition === "bottom_left" || dockPosition === "top_left";

  const buttonElement = (
    <div className="relative flex size-6 shrink-0 items-center justify-center">
      {(isRecording || isProcessing) && (
        <span
          key={`${state}-${recording}`}
          className={`pointer-events-none absolute inset-0 animate-od-ping rounded-full ${
            isProcessing ? "bg-[#EFECF0]/40" : "bg-[#F354D0]/50"
          }`}
        />
      )}
      <button
        type="button"
        onClick={toggle}
        // Do not let the click move the keyboard focus into the overlay.
        // The dock lives on every workspace, and a window that holds the
        // focus keeps it across workspace switches, so the focus would stay
        // on the workspace the user just left.
        onMouseDown={(event) => event.preventDefault()}
        disabled={busy || isProcessing}
        className={`relative z-10 flex size-6 cursor-pointer items-center justify-center rounded-full shadow-lg ring-1 ${live ? "bg-[#1A1240]/95" : "bg-white/90"} ${tint}`}
        aria-label={canStop ? "Stop recording" : "Start recording"}
        title={canStop ? "Stop recording (Ctrl+K)" : "Start recording (Ctrl+K)"}
      >
        {content}
      </button>
    </div>
  );

  const pillElement = pillLabel ? (
    <span
      className={`flex min-w-0 items-center gap-2 rounded-full px-3 py-1.5 text-[11px] leading-none font-semibold text-white shadow-lg ring-1 ${
        isRecording
          ? "bg-[#1A1240]/95 ring-[#F354D0]/80"
          : isListening
            ? "bg-[#1A1240]/95 ring-[#A594FF]/80"
            : isProcessing
              ? "bg-[#1A1240]/95 ring-[#EFECF0]/80"
              : "bg-[#1A1240]/90 ring-white/10"
      }`}
    >
      {(isRecording || isListening || isProcessing) && (
        <span
          className={`size-2 shrink-0 animate-pulse rounded-full ${
            isRecording ? "bg-[#F354D0]" : isListening ? "bg-[#A594FF]" : "bg-[#EFECF0]"
          }`}
        />
      )}
      <span className="min-w-0 truncate">{pillLabel}</span>
      {(isRecording || isProcessing) && (
        <span className="flex h-3 shrink-0 items-end gap-[1.5px]">
          {[0, 1, 2].map((i) => (
            <span
              key={i}
              className={`w-[2px] animate-od-eq origin-bottom ${
                isRecording ? "bg-[#F354D0]" : "bg-[#EFECF0]"
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
  ) : null;

  return (
    <div
      className={`flex h-full w-full items-end gap-2 ${
        isLeftAligned
          ? "justify-start pl-2 pr-3"
          : isRightAligned
            ? "justify-end pl-3 pr-2"
            : "justify-center px-2"
      }`}
    >
      {isLeftAligned ? (
        <>
          {buttonElement}
          {pillElement}
        </>
      ) : (
        <>
          {pillElement}
          {buttonElement}
        </>
      )}
    </div>
  );
}
