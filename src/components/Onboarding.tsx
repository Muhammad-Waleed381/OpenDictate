import { useEffect, useState } from "react";
import { useStore } from "@/lib/store";
import * as api from "@/lib/api";
import { formatHotkey } from "@/lib/utils";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { MicTest } from "@/components/MicTest";
import { ModelCard } from "@/components/ModelCard";

const STEPS = ["Mic test", "Models", "Hotkey"] as const;

export function Onboarding() {
  const [step, setStep] = useState(1);
  const [done, setDone] = useState(false);
  const settings = useStore((s) => s.settings);
  const models = useStore((s) => s.models);

  const sttReady = models?.stt_ready ?? false;
  const vadReady = models?.vad_ready ?? false;

  useEffect(() => {
    const unlistenPromise = api.onModelsReady(() => {
      useStore.getState().refreshModels();
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
          <DialogTitle>Welcome to OpenDictate</DialogTitle>
          <DialogDescription>
            A few quick steps before you can start dictating.
          </DialogDescription>
        </DialogHeader>

        <div className="flex items-center gap-2">
          {STEPS.map((label, i) => {
            const n = i + 1;
            const active = n === step;
            const complete = n < step;
            return (
              <div key={label} className="flex flex-1 flex-col gap-1">
                <div
                  className={`h-1 rounded-full ${complete ? "bg-[#3B82F6]" : active ? "bg-[#3B82F6]/60" : "bg-[#334155]"}`}
                />
                <span
                  className={`text-[11px] ${active ? "text-[#F8FAFC]" : complete ? "text-[#64748B]" : "text-[#64748B]/70"}`}
                >
                  {n}. {label}
                </span>
              </div>
            );
          })}
        </div>

        <div className="flex flex-col gap-4 py-2">
          {step === 1 && <MicTest />}
          {step === 2 && <ModelCard />}
          {step === 3 && (
            <div className="flex flex-col items-center gap-3 py-2">
              <Badge className="bg-[#3B82F6]/15 px-3 py-1 text-sm text-[#3B82F6]">
                {formatHotkey(settings?.hotkey ?? "ctrl+alt+space")}
              </Badge>
              <p className="text-center text-sm text-[#64748B]">
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
            Back
          </Button>
          {step < STEPS.length ? (
            <Button
              onClick={() => setStep((s) => Math.min(STEPS.length, s + 1))}
              disabled={step === 2 && !(sttReady && vadReady)}
            >
              Next
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