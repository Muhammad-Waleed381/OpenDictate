import { useEffect, useState } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { useStore } from "@/lib/store";
import * as api from "@/lib/api";
import { formatHotkey, cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Settings, Activity, BookOpen, History, Shield } from "lucide-react";
import { Onboarding } from "@/components/Onboarding";
import { DockButton } from "@/components/DockButton";
import { GeneralTab } from "@/components/tabs/GeneralTab";
import { DictionaryTab } from "@/components/tabs/DictionaryTab";
import { HistoryTab } from "@/components/tabs/HistoryTab";
import { HeatmapTab } from "@/components/tabs/HeatmapTab";
import { PrivacyTab } from "@/components/tabs/PrivacyTab";

const TABS = [
  { id: "general", label: "General", icon: Settings },
  { id: "activity", label: "Activity", icon: Activity },
  { id: "dictionary", label: "Dictionary", icon: BookOpen },
  { id: "history", label: "History", icon: History },
  { id: "privacy", label: "Privacy", icon: Shield },
] as const;

type TabId = (typeof TABS)[number]["id"];

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
  const [tab, setTab] = useState<TabId>("general");
  const [collapsed, setCollapsed] = useState<boolean>(
    () => localStorage.getItem("od:sidebar") !== "collapsed",
  );

  const toggleSidebar = () => {
    setCollapsed((c) => {
      localStorage.setItem("od:sidebar", c ? "collapsed" : "expanded");
      return !c;
    });
  };

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
      <div className="flex min-h-0 flex-1">
        <aside
          className={cn(
            "flex shrink-0 flex-col border-r-2 border-black bg-black transition-[width] duration-200",
            collapsed ? "w-12" : "w-44",
          )}
        >
          <button
            onClick={toggleSidebar}
            title={collapsed ? "Expand sidebar" : "Collapse sidebar"}
            className="flex h-9 shrink-0 cursor-pointer items-center justify-center border-b-2 border-white/10 text-sm text-white/70 transition-colors hover:bg-white/10 hover:text-white"
          >
            {collapsed ? "▸" : "◂"}
          </button>
          <nav className={cn("flex flex-col gap-1", collapsed ? "p-2" : "p-3")}>
            {TABS.map((t) => (
              <button
                key={t.id}
                onClick={() => setTab(t.id)}
                title={collapsed ? t.label : undefined}
                className={cn(
                  "flex cursor-pointer items-center text-xs font-bold tracking-wider uppercase transition-colors duration-150",
                  collapsed ? "justify-center py-2" : "gap-2.5 justify-start px-4 py-2.5",
                  tab === t.id
                    ? "bg-white text-black"
                    : "text-white/70 hover:bg-white/10 hover:text-white",
                )}
              >
                <t.icon className="size-4 shrink-0" strokeWidth={2.5} />
                {!collapsed && t.label}
              </button>
            ))}
          </nav>
          {!collapsed && (
            <div className="mt-auto flex flex-col gap-1 border-t-2 border-white/10 p-4 text-[9px] font-bold tracking-widest text-white/40 uppercase">
              <span>Local-first</span>
              <span>Zero telemetry</span>
            </div>
          )}
        </aside>
        <div className="flex min-w-0 flex-1 flex-col">
          <Header />
          <LastResult />
          <main className="flex-1 overflow-y-auto px-6 py-5">
            <div key={tab} className="animate-od-slide-up">
              {tab === "general" && <GeneralTab />}
              {tab === "activity" && <HeatmapTab />}
              {tab === "dictionary" && <DictionaryTab />}
              {tab === "history" && <HistoryTab />}
              {tab === "privacy" && <PrivacyTab />}
            </div>
          </main>
        </div>
      </div>
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