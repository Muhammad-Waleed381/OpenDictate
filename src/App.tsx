import { useEffect, useState } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { useStore } from "@/lib/store";
import * as api from "@/lib/api";
import { formatHotkey, cn, DEFAULT_HOTKEY } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Settings, Activity, BookOpen, History, FileText, House } from "lucide-react";
import { Onboarding } from "@/components/Onboarding";
import { Toaster } from "@/components/ui/toast";
import { ConfirmDialogHost } from "@/components/ui/confirm-dialog";
import { DockButton } from "@/components/DockButton";
import { useRecording } from "@/lib/useRecording";
import { HomeTab } from "@/components/tabs/HomeTab";
import { SettingsTab } from "@/components/tabs/SettingsTab";
import { DictionaryTab } from "@/components/tabs/DictionaryTab";
import { HistoryTab } from "@/components/tabs/HistoryTab";
import { HeatmapTab } from "@/components/tabs/HeatmapTab";
import { SnippetsTab } from "@/components/tabs/SnippetsTab";

const TABS = [
  { id: "home", label: "Home", icon: House },
  { id: "activity", label: "Activity", icon: Activity },
  { id: "dictionary", label: "Dictionary", icon: BookOpen },
  { id: "snippets", label: "Snippets", icon: FileText },
  { id: "history", label: "History", icon: History },
  { id: "settings", label: "Settings", icon: Settings },
] as const;

type TabId = (typeof TABS)[number]["id"];

function useOpenDictateEvents() {
  useEffect(() => {
    const store = useStore.getState();
    const onDockState = (event: Event) => {
      store.setOverlayState((event as CustomEvent<api.OverlayState>).detail);
    };
    const onDockPartial = (event: Event) => {
      store.setPartial((event as CustomEvent<api.PartialPayload>).detail.text);
    };
    window.addEventListener("opendictate:overlay-state", onDockState);
    window.addEventListener("opendictate:partial", onDockPartial);
    const subs: Promise<UnlistenFn>[] = [
      api.onOverlayState((payload) => store.setOverlayState(payload)),
      api.onAudioLevel((payload) => store.setLevel(payload.rms)),
      api.onModelProgress((payload) => store.addModelProgress(payload)),
      api.onModelsReady(() => store.refreshModels()),
      api.onHistoryUpdated(() =>
        api.getHistory().then((history) => useStore.setState({ history })),
      ),
      api.onHistoryUpdated(() => store.refreshStats()),
      api.onPartial((payload) => store.setPartial(payload.text)),
    ];
    let cancelled = false;
    subs.forEach((sub) => {
      sub.then((unlisten) => {
        if (cancelled) unlisten();
      });
    });
    return () => {
      cancelled = true;
      window.removeEventListener("opendictate:overlay-state", onDockState);
      window.removeEventListener("opendictate:partial", onDockPartial);
    };
  }, []);
}

function RecordingButton() {
  const { recording, toggle } = useRecording();

  return (
    <Button onClick={toggle} variant={recording ? "outline" : "default"} size="sm">
      {recording ? "■ STOP" : "● RECORD"}
    </Button>
  );
}

function Header() {
  const settings = useStore((s) => s.settings);
  const recording = useStore((s) => s.recording);

  return (
    <header className="relative flex items-center gap-3 border-b-2 border-border bg-card px-6 py-3 text-foreground">
      <div className="flex items-center gap-2.5">
        <span className="flex size-5 items-center justify-center border-2 border-border bg-primary text-[10px] font-bold text-primary-foreground">
          OD
        </span>
        <h1 className="text-sm font-bold tracking-[0.2em] uppercase">
          OpenDictate
        </h1>
      </div>
      <Badge variant="outline" className="ml-auto border-border text-foreground shadow-none">
        {formatHotkey(settings?.hotkey ?? DEFAULT_HOTKEY)}
      </Badge>
      <span className="flex items-center gap-1.5 text-[11px] font-bold uppercase tracking-wider">
        <span
          className={`size-2.5 border border-foreground ${recording ? "animate-od-blink bg-foreground" : "bg-transparent"}`}
        />
        {recording ? "Recording" : "Idle"}
      </span>
      <RecordingButton />
    </header>
  );
}

export function MainApp() {
  const settings = useStore((s) => s.settings);
  const [tab, setTab] = useState<TabId>("home");
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
            "flex shrink-0 flex-col border-r-2 border-sidebar-border bg-sidebar transition-[width] duration-200",
            collapsed ? "w-12" : "w-44",
          )}
        >
          <button
            onClick={toggleSidebar}
            title={collapsed ? "Expand sidebar" : "Collapse sidebar"}
            className="flex h-9 shrink-0 cursor-pointer items-center justify-center border-b-2 border-sidebar-border text-sm text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground"
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
                  "relative flex cursor-pointer items-center text-xs font-bold tracking-wider uppercase transition-colors duration-150",
                  collapsed ? "justify-center py-2" : "gap-2.5 justify-start px-4 py-2.5",
                  tab === t.id
                    ? "bg-brand text-brand-foreground"
                    : "text-sidebar-foreground/70 hover:bg-sidebar-accent hover:text-sidebar-foreground",
                )}
              >
                {tab === t.id && (
                  <span className="absolute inset-y-0 left-0 w-1 bg-brand-foreground" aria-hidden />
                )}
                <t.icon className="size-4 shrink-0" strokeWidth={2.5} />
                {!collapsed && t.label}
              </button>
            ))}
          </nav>
          {!collapsed && (
            <div className="mt-auto flex flex-col gap-1 border-t-2 border-sidebar-border p-4 text-[9px] font-bold tracking-widest text-muted-foreground uppercase">
              <span>Local-first</span>
              <span>Zero telemetry</span>
            </div>
          )}
        </aside>
        <div className="flex min-w-0 flex-1 flex-col">
          <Header />
          <main className="flex-1 overflow-y-auto px-6 py-5">
            <div key={tab} className="animate-od-slide-up">
              {tab === "home" && <HomeTab />}
              {tab === "activity" && <HeatmapTab />}
              {tab === "dictionary" && <DictionaryTab />}
              {tab === "snippets" && <SnippetsTab />}
              {tab === "history" && <HistoryTab />}
              {tab === "settings" && <SettingsTab />}
            </div>
          </main>
        </div>
      </div>
      <footer className="flex items-center gap-3 border-t-2 border-border bg-card px-6 py-2.5 text-[11px] font-bold tracking-wider text-foreground uppercase">
        <span>Speak. Don't type.</span>
        <span className="ml-auto hidden text-muted-foreground sm:inline">
          Local-first · zero telemetry · MIT
        </span>
        <span className="text-muted-foreground tabular-nums">v0.1.0</span>
      </footer>
      {settings && !settings.onboarded && <Onboarding />}
      <Toaster />
      <ConfirmDialogHost />
    </div>
  );
}

export function DockApp() {
  useOpenDictateEvents();

  return (
    <div className="fixed inset-x-0 bottom-0 h-[29px] w-full overflow-hidden">
      <DockButton />
    </div>
  );
}
