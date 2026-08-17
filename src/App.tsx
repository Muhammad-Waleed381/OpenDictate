import { useEffect, useState } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { useStore } from "@/lib/store";
import * as api from "@/lib/api";
import { formatHotkey } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Onboarding } from "@/components/Onboarding";
import { DockButton } from "@/components/DockButton";
import { GeneralTab } from "@/components/tabs/GeneralTab";
import { DictionaryTab } from "@/components/tabs/DictionaryTab";
import { HistoryTab } from "@/components/tabs/HistoryTab";
import { HeatmapTab } from "@/components/tabs/HeatmapTab";
import { PrivacyTab } from "@/components/tabs/PrivacyTab";

function useOpenDictateEvents() {
  useEffect(() => {
    const store = useStore.getState();
    const subs: Promise<UnlistenFn>[] = [
      api.onOverlayState((payload) => store.setOverlayState(payload)),
      api.onAudioLevel((payload) => store.setLevel(payload.rms)),
      api.onModelProgress((payload) => store.addModelProgress(payload)),
      api.onModelsReady(() => store.refreshModels()),
      api.onHistoryUpdated(() =>
        api.getHistory().then((history) => useStore.setState({ history })),
      ),
      api.onHistoryUpdated(() => store.refreshStats()),
    ];
    let cancelled = false;
    subs.forEach((sub) => {
      sub.then((unlisten) => {
        if (cancelled) unlisten();
      });
    });
    return () => {
      cancelled = true;
    };
  }, []);
}

function RecordingButton() {
  const recording = useStore((s) => s.recording);
  const [error, setError] = useState<string | null>(null);

  const handleClick = async () => {
    setError(null);
    if (recording) {
      try {
        const result = await api.stopRecording();
        if (result?.text) {
          useStore.setState({ lastResult: result });
        }
      } catch (e) {
        setError(String(e));
      }
      useStore.getState().setRecording(false);
    } else {
      try {
        await api.startRecording("dictate");
        useStore.getState().setRecording(true);
      } catch (e) {
        setError(String(e));
      }
    }
  };

  return (
    <>
      <Button onClick={handleClick} variant={recording ? "outline" : "default"} size="sm">
        {recording ? "■ STOP" : "● RECORD"}
      </Button>
      {error && (
        <span className="absolute right-6 top-full z-10 mt-1 max-w-xs border-2 border-white bg-black px-2 py-1 text-[10px] font-bold tracking-wider text-white uppercase">
          ✕ {error}
        </span>
      )}
    </>
  );
}

function Header() {
  const settings = useStore((s) => s.settings);
  const recording = useStore((s) => s.recording);

  return (
    <header className="relative flex items-center gap-3 border-b-2 border-black bg-black px-6 py-3 text-white">
      <div className="flex items-center gap-2.5">
        <span className="flex size-5 items-center justify-center border-2 border-white bg-white text-[10px] font-bold text-black">
          OD
        </span>
        <h1 className="text-sm font-bold tracking-[0.2em] uppercase">
          OpenDictate
        </h1>
      </div>
      <Badge variant="outline" className="ml-auto border-white text-white shadow-none">
        {formatHotkey(settings?.hotkey ?? "ctrl+alt+space")}
      </Badge>
      <span className="flex items-center gap-1.5 text-[11px] font-bold uppercase tracking-wider">
        <span
          className={`size-2.5 border border-white ${recording ? "animate-od-blink bg-white" : "bg-transparent"}`}
        />
        {recording ? "Recording" : "Idle"}
      </span>
      <RecordingButton />
    </header>
  );
}

function LastResult() {
  const lastResult = useStore((s) => s.lastResult);

  if (!lastResult) return null;

  return (
    <div className="animate-od-slide-up flex items-center gap-3 border-b-2 border-black px-6 py-2.5">
      <span className="flex h-5 shrink-0 items-center border-2 border-black bg-black px-1.5 text-[10px] font-bold tracking-wider text-white">
        INSERTED ✓
      </span>
      <span className="truncate text-sm font-medium">“{lastResult.text}”</span>
      {lastResult.duration_ms > 0 && (
        <span className="ml-auto shrink-0 text-xs font-bold text-muted-foreground tabular-nums">
          {(lastResult.duration_ms / 1000).toFixed(1)}s
        </span>
      )}
    </div>
  );
}

export function MainApp() {
  const settings = useStore((s) => s.settings);

  useOpenDictateEvents();

  useEffect(() => {
    useStore.getState().refreshAll().catch(() => {});
  }, []);

  useEffect(() => {
    const unlistenPromise = api.onTranscript((payload) => {
      useStore.setState({ lastResult: { text: payload.text, duration_ms: 0 } });
    });
    return () => {
      unlistenPromise.then((fn) => fn()).catch(() => {});
    };
  }, []);

  return (
    <div className="flex h-screen flex-col bg-background text-foreground">
      <Header />
      <LastResult />
      <main className="flex-1 overflow-y-auto px-6 py-5">
        <Tabs defaultValue="general">
          <TabsList>
            <TabsTrigger value="general">General</TabsTrigger>
            <TabsTrigger value="activity">Activity</TabsTrigger>
            <TabsTrigger value="dictionary">Dictionary</TabsTrigger>
            <TabsTrigger value="history">History</TabsTrigger>
            <TabsTrigger value="privacy">Privacy</TabsTrigger>
          </TabsList>
          <TabsContent value="general" className="animate-od-slide-up pt-5">
            <GeneralTab />
          </TabsContent>
          <TabsContent value="activity" className="animate-od-slide-up pt-5">
            <HeatmapTab />
          </TabsContent>
          <TabsContent value="dictionary" className="animate-od-slide-up pt-5">
            <DictionaryTab />
          </TabsContent>
          <TabsContent value="history" className="animate-od-slide-up pt-5">
            <HistoryTab />
          </TabsContent>
          <TabsContent value="privacy" className="animate-od-slide-up pt-5">
            <PrivacyTab />
          </TabsContent>
        </Tabs>
      </main>
      <footer className="flex items-center gap-3 border-t-2 border-black bg-black px-6 py-2.5 text-[11px] font-bold tracking-wider text-white uppercase">
        <span>Speak. Don't type.</span>
        <span className="ml-auto hidden text-white/60 sm:inline">
          Local-first · zero telemetry · MIT
        </span>
        <span className="text-white/60 tabular-nums">v0.1.0</span>
      </footer>
      {settings && !settings.onboarded && <Onboarding />}
    </div>
  );
}

export function DockApp() {
  useOpenDictateEvents();

  return (
    <div className="fixed inset-0">
      <DockButton />
    </div>
  );
}