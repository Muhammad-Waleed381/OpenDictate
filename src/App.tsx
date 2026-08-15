import { useEffect } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { useStore } from "@/lib/store";
import * as api from "@/lib/api";
import { formatHotkey } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Onboarding } from "@/components/Onboarding";
import { OverlayPill } from "@/components/OverlayPill";
import { GeneralTab } from "@/components/tabs/GeneralTab";
import { DictionaryTab } from "@/components/tabs/DictionaryTab";
import { HistoryTab } from "@/components/tabs/HistoryTab";
import { PrivacyTab } from "@/components/tabs/PrivacyTab";

function useOpenDictateEvents() {
  useEffect(() => {
    const store = useStore.getState();
    const subs: Promise<UnlistenFn>[] = [
      api.onOverlayState((payload) => store.setOverlayState(payload)),
      api.onAudioLevel((payload) => store.setLevel(payload.rms)),
      api.onModelProgress((payload) => store.addModelProgress(payload)),
      api.onModelsReady(() => store.refreshModels()),
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

  const handleClick = async () => {
    if (recording) {
      try {
        const result = await api.stopRecording();
        if (result.text) {
          useStore.setState({ lastResult: result });
        }
      } catch {}
      useStore.getState().setRecording(false);
    } else {
      try {
        await api.startRecording("dictate");
        useStore.getState().setRecording(true);
      } catch {}
    }
  };

  return (
    <Button
      onClick={handleClick}
      variant={recording ? "destructive" : "default"}
      size="sm"
    >
      {recording ? "Stop" : "Record"}
    </Button>
  );
}

function Header() {
  const settings = useStore((s) => s.settings);
  const recording = useStore((s) => s.recording);

  return (
    <header className="flex items-center gap-3 border-b border-border px-6 py-4">
      <span className="size-2.5 rounded-full bg-[#3B82F6]" />
      <h1 className="text-sm font-semibold tracking-wide text-[#F8FAFC]">
        OpenDictate
      </h1>
      <Badge variant="outline" className="ml-auto font-mono">
        {formatHotkey(settings?.hotkey ?? "ctrl+alt+space")}
      </Badge>
      <span
        className={`flex items-center gap-1.5 text-xs ${recording ? "text-[#3B82F6]" : "text-[#64748B]"}`}
      >
        <span
          className={`size-2 rounded-full ${recording ? "animate-pulse bg-[#3B82F6]" : "bg-[#334155]"}`}
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
    <div className="border-b border-border px-6 py-2 text-xs text-[#64748B]">
      <span className="text-[#10B981]">Inserted</span>{" "}
      <span className="line-clamp-2 text-[#F8FAFC]">
        “{lastResult.text}”
      </span>{" "}
      {lastResult.duration_ms > 0 && (
        <span className="tabular-nums">
          ({(lastResult.duration_ms / 1000).toFixed(1)}s)
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
    <div className="flex h-screen flex-col bg-[#0F172A] text-[#F8FAFC]">
      <Header />
      <LastResult />
      <main className="flex-1 overflow-y-auto px-6 py-5">
        <Tabs defaultValue="general">
          <TabsList>
            <TabsTrigger value="general">General</TabsTrigger>
            <TabsTrigger value="dictionary">Dictionary</TabsTrigger>
            <TabsTrigger value="history">History</TabsTrigger>
            <TabsTrigger value="privacy">Privacy</TabsTrigger>
          </TabsList>
          <TabsContent value="general" className="pt-5">
            <GeneralTab />
          </TabsContent>
          <TabsContent value="dictionary" className="pt-5">
            <DictionaryTab />
          </TabsContent>
          <TabsContent value="history" className="pt-5">
            <HistoryTab />
          </TabsContent>
          <TabsContent value="privacy" className="pt-5">
            <PrivacyTab />
          </TabsContent>
        </Tabs>
      </main>
      <footer className="border-t border-border px-6 py-3 text-xs text-[#64748B]">
        v0.1.0 · Local-first · zero telemetry · MIT
      </footer>
      {settings && !settings.onboarded && <Onboarding />}
    </div>
  );
}

export function OverlayApp() {
  useOpenDictateEvents();

  return (
    <div className="pointer-events-none fixed inset-0 bg-transparent">
      <OverlayPill />
    </div>
  );
}