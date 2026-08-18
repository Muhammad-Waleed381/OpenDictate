import { useEffect, useState } from "react";
import { useStore } from "@/lib/store";
import * as api from "@/lib/api";
import { formatHotkey } from "@/lib/utils";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { MicTest } from "@/components/MicTest";
import { ModelCard } from "@/components/ModelCard";

const STEPS = ["Mic test", "Models", "Hotkey"] as const;

export function Onboarding() {
  const [step, setStep] = useState(1);
  const [done, setDone] = useState(false);
  const settings = useStore((s) => s.settings);
  const models = useStore((s) => s.models);

  const sttReady = models?.stt_ready ?? false;

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

  return (
    <Dialog open={!done} onOpenChange={() => {}} modal>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <div className="flex items-center gap-2">
            <span className="flex size-6 items-center justify-center border-2 border-black bg-black text-xs font-bold text-white">
              OD
            </span>
            <DialogTitle>Setup — 3 steps</DialogTitle>
          </div>
          <DialogDescription>
            A few quick checks before you can start dictating.
          </DialogDescription>
        </DialogHeader>

        <div className="flex items-center gap-0">
          {STEPS.map((label, i) => {
            const n = i + 1;
            const active = n === step;
            const complete = n < step;
            return (
              <div key={label} className="flex flex-1 items-center">
                <div className="flex flex-col items-center gap-1.5">
                  <span
                    className={`flex size-8 items-center justify-center border-2 border-black text-xs font-bold transition-all duration-200 ease-spring ${
                      complete
                        ? "bg-black text-white"
                        : active
                          ? "bg-black text-white shadow-[3px_3px_0_0_#E8E8E8]"
                          : "bg-white text-muted-foreground"
                    }`}
                  >
                    {complete ? "✓" : `0${n}`}
                  </span>
                  <span
                    className={`text-[10px] font-bold uppercase tracking-wider ${
                      active ? "text-foreground" : "text-muted-foreground"
                    }`}
                  >
                    {label}
                  </span>
                </div>
                {i < STEPS.length - 1 && (
                  <span
                    className={`mx-2 mb-5 h-0.5 flex-1 ${
                      complete || (active && n === step) ? "bg-black" : "bg-muted-foreground/40"
                    }`}
                  />
                )}
              </div>
            );
          })}
        </div>

        <div className="flex flex-col gap-4 border-2 border-black bg-white p-4 shadow-[4px_4px_0_0_#E8E8E8]">
          {step === 1 && <MicTest />}
          {step === 2 && <ModelCard />}
          {step === 3 && (
            <div className="flex flex-col items-center gap-4 py-2">
              <span className="border-2 border-black bg-black px-4 py-2 text-lg font-bold tracking-widest text-white uppercase shadow-[4px_4px_0_0_#E8E8E8]">
                {formatHotkey(settings?.hotkey ?? "ctrl+alt+space")}
              </span>
              <p className="text-center text-sm">
                Press this combination anywhere to start dictating. Press it
                again to stop, transcribe, and insert the text into whatever
                app is focused.
              </p>
            </div>
          )}
        </div>

        <div className="flex items-center justify-between gap-2">
          <Button
            variant="ghost"
            onClick={() => setStep((s) => Math.max(1, s - 1))}
            disabled={step === 1}
          >
            ← Back
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