import { useCallback, useEffect, useState } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { useStore } from "@/lib/store";
import * as api from "@/lib/api";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Mic, Square } from "lucide-react";
import { AppSidebar, NAV_ITEMS, type TabId } from "@/components/AppSidebar";
import { Onboarding } from "@/components/Onboarding";
import { Toaster, toast } from "@/components/ui/toast";
import { ConfirmDialogHost } from "@/components/ui/confirm-dialog";
import { DockButton } from "@/components/DockButton";
import { UpdatePrompt } from "@/components/UpdatePrompt";
import { useRecording } from "@/lib/useRecording";
import { HomeTab } from "@/components/tabs/HomeTab";
import { SettingsTab } from "@/components/tabs/SettingsTab";
import { DictionaryTab } from "@/components/tabs/DictionaryTab";
import { HistoryTab } from "@/components/tabs/HistoryTab";
import { HeatmapTab } from "@/components/tabs/HeatmapTab";
import { SnippetsTab } from "@/components/tabs/SnippetsTab";
import { ModelsTab } from "@/components/tabs/ModelsTab";



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
      api.onModelCancelled((payload) => {
        store.finishModel(payload.file);
        store.removeModelProgress("silero-vad-v4");
        store.removeModelProgress("silero_vad");
        store.refreshModelState().catch(() => {});
      }),
      api.onModelError((payload) => {
        store.finishModel(payload.file);
        store.removeModelProgress("silero-vad-v4");
        store.removeModelProgress("silero_vad");
        store.refreshModelState().catch(() => {});
      }),
      api.onModelsReady((payload) => {
        if (payload.file) store.finishModel(payload.file);
        store.removeModelProgress("silero-vad-v4");
        store.removeModelProgress("silero_vad");
        store.refreshModelState().catch(() => {});
      }),
      api.onHistoryUpdated(() =>
        api
          .getHistory()
          .then((history) => useStore.setState({ history }))
          .catch(() => {}),
      ),
      api.onHistoryUpdated(() => store.refreshStats().catch(() => {})),
      api.onPartial((payload) => store.setPartial(payload.text)),
      api.onRecordingChanged((payload) => {
        // The live caption belongs to one recording.
        store.setPartial("");
        store.setRecording(payload.recording);
      }),
      api.onDictationError((payload) => toast.error(payload.message)),
      api.onDockPosition((pos) => {
        const cur = useStore.getState().settings;
        if (cur) {
          useStore.getState().setSettings({ ...cur, dock_position: pos });
        }
      }),
    ];
    let cancelled = false;
    const unlisteners: UnlistenFn[] = [];
    subs.forEach((sub) => {
      sub
        .then((unlisten) => {
          if (cancelled) unlisten();
          else unlisteners.push(unlisten);
        })
        .catch(() => {
          // Event API unavailable (e.g. window teardown): nothing to clean up.
        });
    });
    return () => {
      cancelled = true;
      unlisteners.forEach((unlisten) => unlisten());
      window.removeEventListener("opendictate:overlay-state", onDockState);
      window.removeEventListener("opendictate:partial", onDockPartial);
    };
  }, []);
}

function RecordingButton() {
  const { recording, busy, toggle } = useRecording();

  return (
    <Button
      onClick={toggle}
      disabled={busy}
      size="sm"
      className={recording ? "" : "border-transparent bg-record text-record-foreground hover:bg-record-hover"}
      variant={recording ? "outline" : "default"}
    >
      {recording ? <Square className="size-3 fill-current" aria-hidden /> : <Mic aria-hidden />}
      {recording ? "Stop" : "Record"}
    </Button>
  );
}

function Header({ title, showRecord }: { title: string; showRecord: boolean }) {
  const recording = useStore((s) => s.recording);

  return (
    <header className="flex h-14 shrink-0 items-center gap-3 px-6 text-foreground">
      <h1 className="text-lg font-semibold">{title}</h1>
      <span className="ml-auto flex items-center gap-1.5 text-[13px] text-muted-foreground">
        <span
          className={cn("size-2 rounded-full", recording ? "animate-od-blink bg-record-live" : "bg-input")}
          aria-hidden
        />
        {recording ? "Recording" : "Idle"}
      </span>
      {(showRecord || recording) && <RecordingButton />}
    </header>
  );
}

export function MainApp() {
  const settings = useStore((s) => s.settings);
  // Tabs are deep-linkable via location.hash (#settings, #activity, …).
  const [tab, setTab] = useState<TabId>(() => {
    const h = window.location.hash.slice(1) as TabId;
    return NAV_ITEMS.some((t) => t.id === h) ? h : "home";
  });
  const [collapsed, setCollapsed] = useState<boolean>(
    () => localStorage.getItem("od:sidebar") === "collapsed",
  );

  const switchTab = (t: TabId) => {
    setTab(t);
    window.history.replaceState(null, "", `#${t}`);
  };

  const toggleSidebar = useCallback(() => {
    setCollapsed((c) => {
      localStorage.setItem("od:sidebar", c ? "expanded" : "collapsed");
      return !c;
    });
  }, []);

  useOpenDictateEvents();

  useEffect(() => {
    useStore.getState().refreshAll().catch(() => {});
  }, []);

  // In-app links such as "Change model" navigate by hash.
  useEffect(() => {
    const onHashChange = () => {
      const next = window.location.hash.slice(1) as TabId;
      if (NAV_ITEMS.some((t) => t.id === next)) setTab(next);
    };
    window.addEventListener("hashchange", onHashChange);
    return () => window.removeEventListener("hashchange", onHashChange);
  }, []);

  useEffect(() => {
    const unlistenPromise = api.onTranscript((payload) => {
      // The event is the authoritative outcome of an insertion, so it also
      // carries the recording duration.
      useStore.setState({
        lastResult: { ...payload, duration_ms: payload.duration_ms ?? 0 },
      });
    });
    return () => {
      unlistenPromise.then((fn) => fn()).catch(() => {});
    };
  }, []);

  return (
    <div className="flex h-screen flex-col bg-background text-foreground">
      <div className="flex min-h-0 flex-1">
        <AppSidebar tab={tab} onSelect={switchTab} collapsed={collapsed} onToggle={toggleSidebar} />
        <div className="flex min-w-0 flex-1 flex-col">
          <Header
            title={NAV_ITEMS.find((t) => t.id === tab)?.label ?? "OpenDictate"}
            showRecord={tab !== "home"}
          />
          <main className="flex-1 overflow-y-auto px-6 pb-6">
            <div key={tab} className="animate-od-slide-up">
              {tab === "home" && <HomeTab />}
              {tab === "activity" && <HeatmapTab />}
              {tab === "dictionary" && <DictionaryTab />}
              {tab === "snippets" && <SnippetsTab />}
              {tab === "history" && <HistoryTab />}
              {tab === "models" && <ModelsTab />}
              {tab === "settings" && <SettingsTab />}
            </div>
          </main>
        </div>
      </div>
      {settings && !settings.onboarded && <Onboarding />}
      {settings?.onboarded && settings.check_updates !== false && <UpdatePrompt />}
      <Toaster />
      <ConfirmDialogHost />
    </div>
  );
}

export function DockApp() {
  useOpenDictateEvents();

  useEffect(() => {
    api.getSettings().then((s) => useStore.getState().setSettings(s)).catch(() => {});
  }, []);

  return (
    <div className="fixed inset-x-0 bottom-0 h-[29px] w-full overflow-hidden">
      <DockButton />
    </div>
  );
}
