import { useEffect, useRef, useState } from "react";
import { useStore } from "@/lib/store";
import * as api from "@/lib/api";
import { formatHotkey, DEFAULT_HOTKEY } from "@/lib/utils";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { MicTest } from "@/components/MicTest";
import { ModelCard } from "@/components/ModelCard";

const STEPS = ["Microphone", "Speech model", "Shortcut"] as const;

export function Onboarding() {
  const [step, setStep] = useState(1);
  const bodyRef = useRef<HTMLDivElement>(null);
  const [done, setDone] = useState(false);
  const settings = useStore((s) => s.settings);
  const models = useStore((s) => s.models);

  const sttReady = models?.stt_ready ?? false;

  // Only the step body scrolls now; each step should open at its top, not
  // wherever the previous step was scrolled to.
  useEffect(() => {
    bodyRef.current?.scrollTo({ top: 0 });
  }, [step]);

  useEffect(() => {
    const unlistenPromise = api.onModelsReady(() => {
      useStore.getState().refreshModels();
      useStore.getState().refreshCatalog();
      setStep((s) => (s === 2 ? 3 : s));
    });
    return () => {
      unlistenPromise.then((fn) => fn()).catch(() => {});
    };
  }, []);

  const handleDone = async () => {
    setDone(true);
    try {
      await api.completeOnboarding();
      await useStore.getState().refreshAll();
    } catch {
      setDone(false);
    }
  };

  // ✕ and Escape route through onOpenChange; treat them as skip so the
  // modal backdrop can never swallow every click with no way out.
  const handleDismiss = () => {
    void handleDone();
  };

  return (
    <Dialog open={!done} onOpenChange={(open) => { if (!open) handleDismiss(); }} modal>
      {/* Header and footer are pinned; only the step body scrolls, so the
          Back / Skip / Next row stays reachable however long the model list
          gets. p-0 moves padding onto the three sections so the scroll edge
          runs flush. */}
      <DialogContent className="flex max-h-[85dvh] flex-col gap-0 overflow-hidden p-0 sm:max-w-md">
        <div className="flex flex-col gap-4 p-5 pb-4">
          <DialogHeader>
            <div className="flex items-center gap-2">
              <span className="flex size-6 shrink-0 items-center justify-center border-2 border-primary bg-primary text-xs font-bold text-primary-foreground">
                OD
              </span>
              <DialogTitle>Setup — 3 quick steps</DialogTitle>
            </div>
            <DialogDescription>
              Test your microphone, choose how speech is recognized, and learn your shortcut.
            </DialogDescription>
          </DialogHeader>

          <div className="flex items-center gap-0">
            {STEPS.map((label, i) => {
              const n = i + 1;
              const active = n === step;
              const complete = n < step;
              return (
                <div key={label} className="flex min-w-0 flex-1 items-center">
                  <div className="flex min-w-0 flex-col items-center gap-1.5">
                    <span
                      className={`flex size-8 shrink-0 items-center justify-center border-2 border-primary text-xs font-bold transition-all duration-200 ease-spring ${
                        complete
                          ? "bg-primary text-primary-foreground"
                          : active
                            ? "bg-primary text-primary-foreground shadow-brutal"
                            : "bg-card text-muted-foreground"
                      }`}
                    >
                      {complete ? "✓" : `0${n}`}
                    </span>
                    <span
                      className={`max-w-full truncate text-[10px] font-bold uppercase tracking-wider ${
                        active ? "text-foreground" : "text-muted-foreground"
                      }`}
                    >
                      {label}
                    </span>
                  </div>
                  {i < STEPS.length - 1 && (
                    <span
                      className={`mx-2 mb-5 h-0.5 min-w-2 flex-1 ${
                        complete || (active && n === step) ? "bg-primary" : "bg-muted-foreground/40"
                      }`}
                    />
                  )}
                </div>
              );
            })}
          </div>
        </div>

        <div ref={bodyRef} className="min-h-0 flex-1 overflow-y-auto px-5 pb-1">
          <div className="flex flex-col gap-4 border-2 border-border bg-card p-4 shadow-brutal">
            {step === 1 && <MicTest />}
            {step === 2 && <ModelCard />}
            {step === 3 && (
              <div className="flex flex-col items-center gap-4 py-2">
                <span className="border-2 border-primary bg-primary px-4 py-2 text-lg font-bold tracking-widest text-primary-foreground uppercase shadow-brutal">
                  {formatHotkey(settings?.hotkey ?? DEFAULT_HOTKEY)}
                </span>
                <p className="text-center text-sm">
                  Press this combination anywhere to start dictating. Press it
                  again to stop, transcribe, and insert the text into whatever
                  app is focused.
                </p>
              </div>
            )}
          </div>
        </div>

        <div className="mt-4 flex items-center justify-between gap-2 border-t-2 border-border bg-muted p-4">
          <Button
            variant="ghost"
            onClick={() => setStep((s) => Math.max(1, s - 1))}
            disabled={step === 1}
          >
            ← Back
          </Button>
          <Button variant="ghost" className="text-muted-foreground" onClick={handleDone}>
            Skip setup →
          </Button>
          {step < STEPS.length ? (
            <Button
              onClick={() => setStep((s) => Math.min(STEPS.length, s + 1))}
              disabled={step === 2 && !sttReady}
            >
              Next →
            </Button>
          ) : (
            <Button onClick={handleDone} disabled={done}>
              Done
            </Button>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
