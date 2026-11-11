import { useEffect, useState } from "react";
import * as api from "@/lib/api";
import { useStore } from "@/lib/store";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { Progress } from "@/components/ui/progress";
import { Switch } from "@/components/ui/switch";
import { toast } from "@/components/ui/toast";

// The app can sit in the tray for days, so opening it is not the only check.
const RECHECK_MS = 12 * 60 * 60 * 1000;

function openUpdatePage() {
  api.openUpdatePage().catch((e) => toast.error(`Could not open the download page: ${String(e)}`));
}

type InstallState =
  | { kind: "idle" }
  | { kind: "working"; stage: api.UpdateProgress["stage"]; downloaded: number; total: number }
  | { kind: "failed"; message: string };

/** Runs the in-app install and follows its progress. */
function useInstall() {
  const [state, setState] = useState<InstallState>({ kind: "idle" });

  useEffect(() => {
    let active = true;
    let unlisten: (() => void) | undefined;
    api
      .onUpdateProgress((p) => {
        if (active) setState({ kind: "working", ...p });
      })
      .then((off) => (active ? (unlisten = off) : off()))
      .catch(() => {});
    return () => {
      active = false;
      unlisten?.();
    };
  }, []);

  const start = () => {
    setState({ kind: "working", stage: "downloading", downloaded: 0, total: 0 });
    // On success the app restarts, so there is nothing to do afterwards.
    api.installUpdate().catch((e) => setState({ kind: "failed", message: String(e) }));
  };

  return { state, start };
}

function progressText(state: Extract<InstallState, { kind: "working" }>): string {
  if (state.stage === "installing") return "Installing. The app will restart.";
  if (state.total > 0) {
    return `Downloading… ${Math.min(100, Math.round((state.downloaded / state.total) * 100))}%`;
  }
  return "Downloading…";
}

function InstallProgress({ state }: { state: Extract<InstallState, { kind: "working" }> }) {
  const percent =
    state.stage === "installing"
      ? 100
      : state.total > 0
        ? Math.min(100, (state.downloaded / state.total) * 100)
        : null;
  return (
    <div className="flex flex-col gap-1.5" role="status">
      <Progress value={percent} aria-label="Update progress" />
      <p className="text-xs text-muted-foreground">{progressText(state)}</p>
    </div>
  );
}

/** Asks once when the app opens, then quietly: a failed check is not worth a message. */
export function UpdatePrompt() {
  const [update, setUpdate] = useState<api.UpdateInfo | null>(null);
  const [dismissed, setDismissed] = useState<string | null>(null);
  const install = useInstall();

  useEffect(() => {
    let active = true;
    const check = () => {
      api
        .checkForUpdate()
        .then((info) => {
          if (active && info.update_available) setUpdate(info);
        })
        .catch(() => {});
    };
    check();
    const timer = window.setInterval(check, RECHECK_MS);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, []);

  if (!update || update.latest_version === dismissed) return null;

  return (
    <aside
      aria-label="Update available"
      className="animate-od-slide-up fixed right-4 bottom-4 z-40 flex w-72 flex-col gap-3 rounded-xl border bg-popover p-4 text-popover-foreground shadow-lg"
    >
      <div className="flex flex-col gap-1">
        <h2 className="text-sm font-semibold">Update available</h2>
        <p className="text-[13px] text-muted-foreground">
          Version {update.latest_version} is out. You have {update.current_version}.
        </p>
      </div>
      {install.state.kind === "working" && <InstallProgress state={install.state} />}
      {install.state.kind === "failed" && (
        <p className="text-xs text-destructive" role="alert">
          {install.state.message}
        </p>
      )}
      {install.state.kind !== "working" && (
        <div className="flex justify-end gap-2">
          <Button size="sm" variant="ghost" onClick={() => setDismissed(update.latest_version)}>
            Later
          </Button>
          {update.can_install && install.state.kind !== "failed" ? (
            <Button size="sm" onClick={install.start}>
              Update now
            </Button>
          ) : (
            <Button size="sm" onClick={openUpdatePage}>
              Download
            </Button>
          )}
        </div>
      )}
    </aside>
  );
}

type CheckState =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "failed" }
  | { kind: "done"; info: api.UpdateInfo };

export function UpdateSettings() {
  const settings = useStore((s) => s.settings);
  const [check, setCheck] = useState<CheckState>({ kind: "idle" });
  const install = useInstall();

  const runCheck = () => {
    setCheck({ kind: "checking" });
    api
      .checkForUpdate()
      .then((info) => setCheck({ kind: "done", info }))
      .catch(() => setCheck({ kind: "failed" }));
  };

  const setAutomatic = async (check_updates: boolean) => {
    try {
      await api.setSettings({ check_updates });
      await useStore.getState().refreshAll();
    } catch (e) {
      toast.error(`Could not save the setting: ${String(e)}`);
    }
  };

  const available = check.kind === "done" && check.info.update_available;

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-center justify-between gap-2">
        <div className="flex flex-col gap-1">
          <Label>Updates</Label>
          <p className="text-xs text-muted-foreground" role="status">
            {check.kind === "idle" && `You have version ${__APP_VERSION__}.`}
            {check.kind === "checking" && "Checking…"}
            {check.kind === "failed" && "Could not check. Check your connection and try again."}
            {check.kind === "done" &&
              (check.info.update_available
                ? `Version ${check.info.latest_version} is available. You have ${check.info.current_version}.`
                : `You have the latest version, ${check.info.current_version}.`)}
          </p>
        </div>
        {available ? (
          install.state.kind === "working" ? null : check.kind === "done" &&
            check.info.can_install &&
            install.state.kind !== "failed" ? (
            <Button size="sm" onClick={install.start}>
              Download and install
            </Button>
          ) : (
            <Button size="sm" onClick={openUpdatePage}>
              Download
            </Button>
          )
        ) : (
          <Button size="sm" variant="outline" disabled={check.kind === "checking"} onClick={runCheck}>
            Check for updates
          </Button>
        )}
      </div>
      {install.state.kind === "working" && <InstallProgress state={install.state} />}
      {install.state.kind === "failed" && (
        <p className="text-xs text-destructive" role="alert">
          {install.state.message} You can still download it from the release page.
        </p>
      )}
      <div className="flex items-center justify-between gap-2">
        <div className="flex flex-col gap-1">
          <Label htmlFor="check-updates">Check when the app opens</Label>
          <p className="text-xs text-muted-foreground">
            Asks GitHub for the newest version number. Nothing else is sent.
          </p>
        </div>
        <Switch
          id="check-updates"
          checked={settings?.check_updates ?? true}
          onCheckedChange={setAutomatic}
        />
      </div>
    </div>
  );
}
