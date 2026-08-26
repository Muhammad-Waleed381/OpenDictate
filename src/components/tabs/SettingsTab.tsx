import { useEffect, useRef, useState } from "react";
import type { KeyboardEvent } from "react";
import { useStore } from "@/lib/store";
import * as api from "@/lib/api";
import { formatHotkey, DEFAULT_HOTKEY, DOUBLE_TAP_OPTIONS, IS_MAC } from "@/lib/utils";
import { Label } from "@/components/ui/label";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { Slider } from "@/components/ui/slider";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { ModelCard } from "@/components/ModelCard";
import { useTheme } from "@/lib/theme";
import { toast } from "@/components/ui/toast";
import { confirmDialog } from "@/components/ui/confirm-dialog";

const KEY_NAMES: Record<string, string> = {
  " ": "space",
  Enter: "enter",
  Tab: "tab",
  ArrowUp: "up",
  ArrowDown: "down",
  ArrowLeft: "left",
  ArrowRight: "right",
  Home: "home",
  End: "end",
  PageUp: "pageup",
  PageDown: "pagedown",
  Insert: "insert",
  Delete: "delete",
  Backspace: "backspace",
  PrintScreen: "printscreen",
};

function comboFromEvent(e: KeyboardEvent): string | null {
  e.preventDefault();
  const mods: string[] = [];
  if (e.metaKey) mods.push("super");
  if (e.ctrlKey) mods.push("ctrl");
  if (e.altKey) mods.push("alt");
  if (e.shiftKey) mods.push("shift");

  const key = e.key;
  let name: string | null = null;
  if (/^F([1-9]|1\d|2[0-4])$/.test(key)) {
    name = key.toLowerCase();
  } else if (/^[a-z0-9]$/i.test(key)) {
    name = key.toLowerCase();
  } else {
    name = KEY_NAMES[key] ?? null;
  }
  if (!name) return null;
  if (mods.length === 0 && !/^f(1[3-9]|2[0-4])$/.test(name)) return null;
  return [...mods, name].join("+");
}

const GPU_MODES = [
  { value: "off", label: "Off (CPU)" },
  { value: "auto", label: "Auto" },
  { value: "cuda", label: "CUDA (NVIDIA)" },
  { value: "coreml", label: "CoreML (Apple)" },
] as const;

function HotkeyCapture() {
  const settings = useStore((s) => s.settings);
  const [capturing, setCapturing] = useState(false);
  const [preview, setPreview] = useState<string | null>(null);
  const [applied, setApplied] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  const current = formatHotkey(settings?.hotkey ?? DEFAULT_HOTKEY);

  useEffect(() => {
    if (capturing) inputRef.current?.focus();
  }, [capturing]);

  const handleKeyDown = async (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Escape") {
      setCapturing(false);
      setPreview(null);
      setError(null);
      return;
    }
    const combo = comboFromEvent(e);
    if (!combo) {
      setError("Unsupported key — try letters, digits, F1-F24, or arrows with a modifier");
      return;
    }
    setError(null);
    setPreview(combo);
    await applyHotkey(combo);
  };

  const applyHotkey = async (combo: string) => {
    try {
      await api.setSettings({ hotkey: combo });
      await useStore.getState().refreshAll();
      setCapturing(false);
      setError(null);
      setApplied(true);
      setTimeout(() => setApplied(false), 1500);
    } catch (err) {
      setError(String(err));
    }
  };

  return (
    <div className="flex flex-col gap-2">
      <Label htmlFor="hotkey">Global hotkey</Label>
      <div className="flex items-center gap-2">
        <div
          className={`flex h-10 flex-1 items-center border-2 border-border bg-card px-3 font-bold tracking-widest uppercase ${
            capturing ? "animate-od-blink" : ""
          }`}
          onClick={() => setCapturing(true)}
        >
          {capturing ? (preview ?? "Press your combination…") : current}
          <input
            ref={inputRef}
            id="hotkey"
            className="sr-only"
            value=""
            readOnly
            onKeyDown={handleKeyDown}
            aria-label="Press a key combination"
          />
        </div>
        {capturing ? (
          <Button variant="outline" onClick={() => { setCapturing(false); setPreview(null); setError(null); }}>
            Cancel
          </Button>
        ) : (
          <Button onClick={() => { setCapturing(true); setError(null); }}>
            {applied ? "✓ Captured" : "Capture"}
          </Button>
        )}
      </div>
      {IS_MAC && (
        <div className="flex flex-col gap-1.5">
          <span className="text-[10px] font-bold uppercase tracking-wider text-muted-foreground">
            Or double-tap a modifier
          </span>
          <div className="flex flex-wrap gap-1.5">
            {DOUBLE_TAP_OPTIONS.map((option) => (
              <Button
                key={option.value}
                size="sm"
                variant={settings?.hotkey === option.value ? "default" : "outline"}
                onClick={() => applyHotkey(option.value)}
              >
                {option.label}
              </Button>
            ))}
          </div>
        </div>
      )}
      <p className="text-xs text-muted-foreground">
        {capturing
          ? "Press the keys now — Escape cancels."
          : applied
            ? "Hotkey saved. Works anywhere, even when the app is in the tray."
            : "Click Capture, then press your combination."}
      </p>
      {error && (
        <div className="border-2 border-primary bg-primary px-2 py-1.5 text-xs font-bold text-primary-foreground uppercase">
          ✕ {error}
        </div>
      )}
    </div>
  );
}

const micLabel = (mic: string | null, mics: api.MicDevice[]): string => {
  if (mic === null) return "Select microphone";
  const found = mics.find((m) => m.id === mic);
  return found?.label ?? mic;
};

export function SettingsTab() {
  const mics = useStore((s) => s.mics);
  const mic = useStore((s) => s.mic);
  const settings = useStore((s) => s.settings);
  const toggleRequest = useRef(0);
  const themePref = useTheme((s) => s.pref);
  const setThemePref = useTheme((s) => s.setPref);

  const handleMicChange = async (name: string | null) => {
    if (!name) return;
    try {
      await api.setMic(name);
      useStore.getState().setMic(name);
    } catch (e) {
      toast.error(`Microphone switch failed: ${String(e)}`);
    }
  };

  const handleLanguageChange = async (language: string | null) => {
    if (!language) return;
    try {
      await api.setSettings({ language });
      useStore.getState().refreshAll();
    } catch (e) {
      toast.error(`Language change failed: ${String(e)}`);
    }
  };

  const persistToggle = async (
    key: "continuous" | "hold_to_talk" | "autostart" | "spoken_punctuation" | "audio_feedback",
    enabled: boolean,
  ) => {
    const current = useStore.getState().settings;
    if (!current) return;
    const request = ++toggleRequest.current;
    useStore.getState().setSettings({ ...current, [key]: enabled });
    try {
      await api.setSettings({ [key]: enabled });
    } catch {
      if (request === toggleRequest.current) {
        const latest = await api.getSettings().catch(() => null);
        if (latest) useStore.getState().setSettings(latest);
        toast.error("Could not save setting — reverted");
      }
    }
  };

  const handleContinuousChange = (enabled: boolean) => persistToggle("continuous", enabled);
  const handleHoldToTalkChange = (enabled: boolean) => persistToggle("hold_to_talk", enabled);
  const handleAutostartChange = (enabled: boolean) => persistToggle("autostart", enabled);
  const handleSpokenPunctuationChange = (enabled: boolean) =>
    persistToggle("spoken_punctuation", enabled);
  const handleAudioFeedbackChange = (enabled: boolean) =>
    persistToggle("audio_feedback", enabled);

  const handlePlaySound = async (event: string) => {
    try {
      await api.playTestSound(event, settings?.audio_feedback_volume);
    } catch (e) {
      toast.error(`Sound playback failed: ${String(e)}`);
    }
  };

  const handleResetSettings = async () => {
    const ok = await confirmDialog({
      title: "Reset all settings to default?",
      description:
        "All shortcut combinations, dictation preferences, and audio settings will be reset to factory defaults.",
      confirmLabel: "Reset to default",
      destructive: true,
    });
    if (!ok) return;
    try {
      const defaults = await api.resetSettings();
      useStore.getState().setSettings(defaults);
      toast.success("Settings reset to factory defaults");
      await useStore.getState().refreshAll();
    } catch (e) {
      toast.error(`Reset failed: ${String(e)}`);
    }
  };

  const handleGpuChange = async (mode: string | null) => {
    if (!mode) return;
    const current = useStore.getState().settings;
    if (!current) return;
    useStore.getState().setSettings({ ...current, gpu: mode });
    try {
      await api.setSettings({ gpu: mode });
    } catch {
      const latest = await api.getSettings().catch(() => null);
      if (latest) useStore.getState().setSettings(latest);
      toast.error("Could not save GPU setting — reverted");
    }
  };

  const volumeTimer = useRef<number | null>(null);

  const handleVolumeChange = (value: number) => {
    const current = useStore.getState().settings;
    if (!current) return;
    useStore.getState().setSettings({ ...current, audio_feedback_volume: value });
    if (volumeTimer.current) window.clearTimeout(volumeTimer.current);
    volumeTimer.current = window.setTimeout(async () => {
      try {
        await api.setSettings({ audio_feedback_volume: value });
      } catch (e) {
        toast.error(`Could not save volume: ${String(e)}`);
      }
    }, 150);
  };

  return (
    <div className="flex flex-col gap-6">
      <div className="flex items-center justify-between gap-2">
        <div className="flex flex-col gap-1">
          <Label htmlFor="theme-select">Appearance</Label>
          <p className="text-xs text-muted-foreground">
            Follow the system setting or pick light / dark.
          </p>
        </div>
        <Select value={themePref} onValueChange={(v) => setThemePref(v as "system" | "light" | "dark")}>
          <SelectTrigger id="theme-select" className="w-32">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="system">System</SelectItem>
            <SelectItem value="light">Light</SelectItem>
            <SelectItem value="dark">Dark</SelectItem>
          </SelectContent>
        </Select>
      </div>
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
        <div className="flex flex-col gap-2">
          <Label htmlFor="mic">Microphone</Label>
          <Select value={mic ?? ""} onValueChange={handleMicChange}>
            <SelectTrigger className="w-full" id="mic">
              <SelectValue>{micLabel(mic, mics)}</SelectValue>
            </SelectTrigger>
            <SelectContent className="w-full">
              {mics.length === 0 ? (
                <SelectItem value="__none__" disabled>
                  No microphones found
                </SelectItem>
              ) : (
                mics.map((device) => (
                  <SelectItem key={device.id} value={device.id}>
                    {device.label}
                  </SelectItem>
                ))
              )}
            </SelectContent>
          </Select>
        </div>

        <div className="flex flex-col gap-2">
          <Label htmlFor="language">Language</Label>
          <Select
            value={settings?.language ?? "auto"}
            onValueChange={handleLanguageChange}
          >
            <SelectTrigger className="w-full" id="language">
              <SelectValue>
                {settings?.language === "auto" || settings?.language == null
                  ? "Auto (system default)"
                  : settings.language}
              </SelectValue>
            </SelectTrigger>
            <SelectContent className="w-full">
              <SelectItem value="auto">Auto (system default)</SelectItem>
              <SelectItem value="en">English</SelectItem>
              <SelectItem value="es">Spanish</SelectItem>
              <SelectItem value="fr">French</SelectItem>
              <SelectItem value="de">German</SelectItem>
              <SelectItem value="pt">Portuguese</SelectItem>
              <SelectItem value="it">Italian</SelectItem>
              <SelectItem value="ur">Urdu</SelectItem>
              <SelectItem value="hi">Hindi</SelectItem>
            </SelectContent>
          </Select>
        </div>

        <div className="sm:col-span-2">
          <HotkeyCapture />
        </div>
      </div>

      <div className="flex items-center justify-between gap-2">
        <div className="flex flex-col gap-1">
          <Label htmlFor="continuous">Continuous dictation</Label>
          <p className="text-xs text-muted-foreground">
            Keep listening after each phrase — press the hotkey again to stop.
          </p>
        </div>
        <Switch
          id="continuous"
          checked={settings?.continuous ?? false}
          onCheckedChange={handleContinuousChange}
        />
      </div>

      <div className="flex items-center justify-between gap-2">
        <div className="flex flex-col gap-1">
          <Label htmlFor="hold-to-talk">Hold-to-talk mode</Label>
          <p className="text-xs text-muted-foreground">
            Hold your global hotkey while speaking and release to type.
          </p>
        </div>
        <Switch
          id="hold-to-talk"
          checked={settings?.hold_to_talk ?? false}
          onCheckedChange={handleHoldToTalkChange}
        />
      </div>

      <div className="flex items-center justify-between gap-2">
        <div className="flex flex-col gap-1">
          <Label htmlFor="gpu">GPU acceleration</Label>
          <p className="text-xs text-muted-foreground">
            Experimental. Engines silently fall back to CPU when a provider is
            unavailable, and changes apply from the next dictation.
          </p>
        </div>
        <Select value={settings?.gpu ?? "off"} onValueChange={handleGpuChange}>
          <SelectTrigger className="w-40">
            <SelectValue>
              {GPU_MODES.find((m) => m.value === (settings?.gpu ?? "off"))?.label ??
                "Off (CPU)"}
            </SelectValue>
          </SelectTrigger>
          <SelectContent>
            {GPU_MODES.map((m) => (
              <SelectItem key={m.value} value={m.value}>
                {m.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

      <div className="flex items-center justify-between gap-2">
        <div className="flex flex-col gap-1">
          <Label htmlFor="autostart">Start with system</Label>
          <p className="text-xs text-muted-foreground">
            Keep OpenDictate ready after you sign in.
          </p>
        </div>
        <Switch
          id="autostart"
          checked={settings?.autostart ?? false}
          onCheckedChange={handleAutostartChange}
        />
      </div>

      <div className="flex items-center justify-between gap-2">
        <div className="flex flex-col gap-1">
          <Label htmlFor="spoken-punctuation">Spoken punctuation</Label>
          <p className="text-xs text-muted-foreground">
            Say “period”, “comma”, “question mark”, or “exclamation point” to insert punctuation.
          </p>
        </div>
        <Switch
          id="spoken-punctuation"
          checked={settings?.spoken_punctuation ?? false}
          onCheckedChange={handleSpokenPunctuationChange}
        />
      </div>

      <div className="flex flex-col gap-3">
        <div className="flex items-center justify-between gap-2">
          <div className="flex flex-col gap-1">
            <Label htmlFor="audio-feedback">Audio feedback</Label>
            <p className="text-xs text-muted-foreground">
              Play sounds when dictation starts, inserts text, or runs into an error.
            </p>
          </div>
          <Switch
            id="audio-feedback"
            checked={settings?.audio_feedback ?? false}
            onCheckedChange={handleAudioFeedbackChange}
          />
        </div>

        {settings?.audio_feedback && (
          <div className="flex flex-col gap-2 rounded border border-border/40 bg-card p-3">
            <div className="flex items-center justify-between gap-4">
              <Label htmlFor="audio-feedback-volume" className="shrink-0 text-xs">
                Sound Volume
              </Label>
              <Slider
                id="audio-feedback-volume"
                className="flex-1"
                min={0}
                max={100}
                value={Math.round((settings?.audio_feedback_volume ?? 0.5) * 100)}
                onChange={(v) => handleVolumeChange(v / 100)}
              />
              <span className="w-12 text-right text-xs font-bold tabular-nums">
                {Math.round((settings?.audio_feedback_volume ?? 0.5) * 100)}%
              </span>
            </div>
            <div className="flex items-center gap-2 pt-1">
              <span className="text-[10px] font-bold text-muted-foreground uppercase">
                Test audio cues:
              </span>
              <Button size="sm" variant="outline" onClick={() => handlePlaySound("start")}>
                ▶ Start
              </Button>
              <Button size="sm" variant="outline" onClick={() => handlePlaySound("insert")}>
                ▶ Insert
              </Button>
              <Button size="sm" variant="outline" onClick={() => handlePlaySound("error")}>
                ▶ Error
              </Button>
            </div>
          </div>
        )}
      </div>

      <ModelCard />

      <div className="mt-4 flex items-center justify-between border-t border-border pt-4">
        <div className="flex flex-col">
          <span className="text-xs font-bold uppercase text-muted-foreground">
            Factory reset
          </span>
          <span className="text-[11px] text-muted-foreground">
            Restore default shortcuts, engine, and preferences.
          </span>
        </div>
        <Button variant="outline" className="text-destructive hover:bg-destructive hover:text-destructive-foreground" onClick={handleResetSettings}>
          Reset settings to default
        </Button>
      </div>
    </div>
  );
}
