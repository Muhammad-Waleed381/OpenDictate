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
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
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
    key:
      | "continuous"
      | "hold_to_talk"
      | "autostart"
      | "spoken_punctuation"
      | "audio_feedback"
      | "handsfree_mode"
      | "voice_actions_enabled",
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
  const handleHandsfreeChange = (enabled: boolean) => persistToggle("handsfree_mode", enabled);
  const handleVoiceActionsChange = (enabled: boolean) =>
    persistToggle("voice_actions_enabled", enabled);

  const modelProgress = useStore((s) => s.modelProgress);
  const [downloadingKws, setDownloadingKws] = useState(false);
  const [wakeWordsInput, setWakeWordsInput] = useState(settings?.wake_words ?? "hey dictate, computer");
  const [groqKeyInput, setGroqKeyInput] = useState(settings?.groq_api_key ?? "");
  const [testingGroq, setTestingGroq] = useState(false);
  const [testGroqResult, setTestGroqResult] = useState<string | null>(null);
  const [showVoiceActionsHelp, setShowVoiceActionsHelp] = useState(false);

  const kwsProgress = modelProgress.find((p) => p.file.includes("kws"));

  const handleDownloadKws = async () => {
    setDownloadingKws(true);
    try {
      toast.info("Downloading Keyword Spotting model (15MB)...");
      await api.ensureModel("kws-zipformer-gigaspeech-3.3m-2024-01-01");
      await useStore.getState().refreshCatalog();
      toast.success("Keyword spotting model downloaded and ready!");
    } catch (e) {
      toast.error(`KWS model download failed: ${String(e)}`);
    } finally {
      setDownloadingKws(false);
    }
  };

  useEffect(() => {
    if (settings?.wake_words !== undefined) {
      setWakeWordsInput(settings.wake_words);
    }
  }, [settings?.wake_words]);

  useEffect(() => {
    if (settings?.groq_api_key !== undefined) {
      setGroqKeyInput(settings.groq_api_key ?? "");
    }
  }, [settings?.groq_api_key]);

  const handleSaveWakeWords = async () => {
    try {
      await api.setSettings({ wake_words: wakeWordsInput });
      toast.success("Wake words saved");
    } catch (e) {
      toast.error(`Failed to save wake words: ${String(e)}`);
    }
  };

  const handleTimeoutChange = async (sec: number) => {
    const current = useStore.getState().settings;
    if (!current) return;
    useStore.getState().setSettings({ ...current, handsfree_silence_timeout_sec: sec });
    try {
      await api.setSettings({ handsfree_silence_timeout_sec: sec });
    } catch (e) {
      toast.error(`Could not save timeout: ${String(e)}`);
    }
  };

  const handlePolishProviderChange = async (provider: string | null) => {
    if (!provider) return;
    const current = useStore.getState().settings;
    if (!current) return;
    useStore.getState().setSettings({
      ...current,
      polish_provider: provider as "off" | "groq" | "local_slm",
    });
    try {
      await api.setSettings({
        polish_provider: provider as "off" | "groq" | "local_slm",
      });
      toast.success(`Voice Polish set to ${provider}`);
    } catch (e) {
      toast.error(`Failed to update Polish provider: ${String(e)}`);
    }
  };

  const handlePolishModeChange = async (mode: string | null) => {
    if (!mode) return;
    const current = useStore.getState().settings;
    if (!current) return;
    useStore.getState().setSettings({
      ...current,
      polish_mode: mode as "clean" | "bullets",
    });
    try {
      await api.setSettings({
        polish_mode: mode as "clean" | "bullets",
      });
    } catch (e) {
      toast.error(`Failed to update Polish mode: ${String(e)}`);
    }
  };

  const handleGroqModelChange = async (model: string | null) => {
    if (!model) return;
    const current = useStore.getState().settings;
    if (!current) return;
    useStore.getState().setSettings({
      ...current,
      groq_model: model,
    });
    try {
      await api.setSettings({ groq_model: model });
    } catch (e) {
      toast.error(`Failed to update Groq model: ${String(e)}`);
    }
  };

  const handleSaveGroqKey = async () => {
    try {
      await api.setSettings({ groq_api_key: groqKeyInput.trim() });
      toast.success("Groq API key saved");
    } catch (e) {
      toast.error(`Failed to save Groq API key: ${String(e)}`);
    }
  };

  const handleTestGroq = async () => {
    if (!groqKeyInput.trim()) {
      toast.error("Please enter a Groq API key first");
      return;
    }
    setTestingGroq(true);
    setTestGroqResult(null);
    try {
      const res = await api.testGroqApiKey(
        groqKeyInput.trim(),
        settings?.groq_model ?? "llama-3.1-8b-instant",
      );
      setTestGroqResult(res);
      toast.success("Groq API key test succeeded! ⚡");
    } catch (e) {
      toast.error(`Groq API test failed: ${String(e)}`);
    } finally {
      setTestingGroq(false);
    }
  };

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

  const models = useStore((s) => s.models);

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

      {/* --- Intelligent Handsfree Mode Card --- */}
      <div className="flex flex-col gap-3 rounded-none border-2 border-border bg-card p-4 shadow-[3px_3px_0_0_var(--od-shadow)]">
        <div className="flex items-center justify-between gap-2">
          <div className="flex flex-col gap-1">
            <div className="flex items-center gap-2">
              <span className="text-sm font-bold uppercase tracking-wider text-foreground">
                🎙️ Intelligent Handsfree Mode
              </span>
              <Badge variant={settings?.handsfree_mode ? "default" : "outline"}>
                {settings?.handsfree_mode ? "Active" : "Disabled"}
              </Badge>
            </div>
            <p className="text-xs text-muted-foreground">
              Always-on, ultra low-power keyword spotting. Say your wake phrase to start dictating without touching the keyboard.
            </p>
          </div>
          <Switch
            id="handsfree-mode"
            checked={settings?.handsfree_mode ?? false}
            onCheckedChange={handleHandsfreeChange}
          />
        </div>

        {settings?.handsfree_mode && (
          <div className="flex flex-col gap-4 border-t-2 border-border/40 pt-3">
            {!models?.kws_ready && (
              <div className="flex flex-col gap-2 bg-amber-500/10 p-3 border border-amber-500/30 text-xs text-amber-600 dark:text-amber-400">
                <div className="flex items-center justify-between gap-2">
                  <span>⚠️ Keyword Spotting (KWS) model required for handsfree (15MB).</span>
                  <Button
                    size="sm"
                    variant="outline"
                    disabled={downloadingKws || !!kwsProgress}
                    onClick={handleDownloadKws}
                  >
                    {downloadingKws || !!kwsProgress ? "Downloading..." : "Download Model (15MB)"}
                  </Button>
                </div>
                {kwsProgress && kwsProgress.total > 0 && (
                  <div className="flex flex-col gap-1 pt-1">
                    <div className="flex items-center justify-between text-[11px] font-mono">
                      <span>Downloading Zipformer KWS model...</span>
                      <span>{Math.round((kwsProgress.received / kwsProgress.total) * 100)}%</span>
                    </div>
                    <div className="h-1.5 w-full bg-amber-500/20 overflow-hidden">
                      <div
                        className="h-full bg-amber-500 transition-all duration-200"
                        style={{ width: `${Math.round((kwsProgress.received / kwsProgress.total) * 100)}%` }}
                      />
                    </div>
                  </div>
                )}
              </div>
            )}

            <div className="flex flex-col gap-1.5">
              <div className="flex items-center justify-between">
                <Label htmlFor="wake-words" className="text-xs font-bold uppercase">
                  Wake Phrases (Comma separated)
                </Label>
                <span className="text-[11px] text-muted-foreground">
                  Default: <code className="bg-muted px-1">hey dictate, computer</code>
                </span>
              </div>
              <div className="flex items-center gap-2">
                <Input
                  id="wake-words"
                  value={wakeWordsInput}
                  onChange={(e) => setWakeWordsInput(e.target.value)}
                  placeholder="e.g. hey dictate, computer, transcribe"
                  className="text-xs"
                />
                <Button size="sm" onClick={handleSaveWakeWords}>
                  Save
                </Button>
              </div>
            </div>

            <div className="flex items-center justify-between gap-4">
              <div className="flex flex-col">
                <Label htmlFor="timeout-slider" className="text-xs font-bold uppercase">
                  Auto-Sleep Inactivity Timeout
                </Label>
                <span className="text-[11px] text-muted-foreground">
                  Puts handsfree back to sleep after silence
                </span>
              </div>
              <div className="flex items-center gap-3 w-48">
                <Slider
                  id="timeout-slider"
                  min={5}
                  max={120}
                  step={5}
                  value={settings?.handsfree_silence_timeout_sec ?? 30}
                  onChange={handleTimeoutChange}
                  className="flex-1"
                />
                <span className="w-10 text-right text-xs font-bold tabular-nums">
                  {settings?.handsfree_silence_timeout_sec ?? 30}s
                </span>
              </div>
            </div>
          </div>
        )}
      </div>

      {/* --- Voice Actions & In-Place Editing Card --- */}
      <div className="flex flex-col gap-3 rounded-none border-2 border-border bg-card p-4 shadow-[3px_3px_0_0_var(--od-shadow)]">
        <div className="flex items-center justify-between gap-2">
          <div className="flex flex-col gap-1">
            <div className="flex items-center gap-2">
              <span className="text-sm font-bold uppercase tracking-wider text-foreground">
                ⚡ Voice Actions & In-Place Editing
              </span>
              <Badge variant={settings?.voice_actions_enabled ? "default" : "outline"}>
                {settings?.voice_actions_enabled ? "Enabled" : "Off"}
              </Badge>
            </div>
            <p className="text-xs text-muted-foreground">
              Speak natural commands like <code className="bg-muted px-1">"scratch that"</code>, <code className="bg-muted px-1">"undo"</code>, <code className="bg-muted px-1">"new paragraph"</code>, or <code className="bg-muted px-1">"all caps &lt;phrase&gt;"</code>.
            </p>
          </div>
          <Switch
            id="voice-actions"
            checked={settings?.voice_actions_enabled ?? true}
            onCheckedChange={handleVoiceActionsChange}
          />
        </div>

        <div className="flex items-center justify-between pt-1">
          <Button
            size="sm"
            variant="outline"
            onClick={() => setShowVoiceActionsHelp(!showVoiceActionsHelp)}
            className="text-xs"
          >
            {showVoiceActionsHelp ? "Hide Voice Commands" : "📖 View Voice Commands Cheat Sheet"}
          </Button>
        </div>

        {showVoiceActionsHelp && (
          <div className="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 gap-2.5 border-t-2 border-border/40 pt-3 text-xs">
            <div className="flex flex-col gap-1 rounded border border-border/60 bg-muted/40 p-2.5">
              <span className="font-bold uppercase text-foreground">🤖 AI Agent Prompt & Send</span>
              <ul className="list-disc pl-4 text-muted-foreground space-y-0.5">
                <li><code className="text-foreground font-semibold">"prompt &lt;text&gt;"</code>: Type & auto-send to Claude/Cursor/ChatGPT/CLI</li>
                <li><code className="text-foreground font-semibold">"send"</code> / <code className="text-foreground font-semibold">"submit"</code>: Press Enter to submit prompt</li>
                <li><code className="text-foreground font-semibold">"stop process"</code> / <code className="text-foreground font-semibold">"cancel command"</code>: Ctrl+C interrupt</li>
              </ul>
            </div>

            <div className="flex flex-col gap-1 rounded border border-border/60 bg-muted/40 p-2.5">
              <span className="font-bold uppercase text-foreground">🔀 App Switching</span>
              <ul className="list-disc pl-4 text-muted-foreground space-y-0.5">
                <li><code className="text-foreground font-semibold">"switch to cursor"</code> / <code className="text-foreground font-semibold">"switch to code"</code></li>
                <li><code className="text-foreground font-semibold">"switch to terminal"</code> / <code className="text-foreground font-semibold">"switch to chrome"</code></li>
                <li><code className="text-foreground font-semibold">"switch to slack"</code> / <code className="text-foreground font-semibold">"switch to files"</code></li>
              </ul>
            </div>

            <div className="flex flex-col gap-1 rounded border border-border/60 bg-muted/40 p-2.5">
              <span className="font-bold uppercase text-foreground">🌐 Voice Search & Tabs</span>
              <ul className="list-disc pl-4 text-muted-foreground space-y-0.5">
                <li><code className="text-foreground font-semibold">"search for &lt;query&gt;"</code> / <code className="text-foreground font-semibold">"google &lt;query&gt;"</code></li>
                <li><code className="text-foreground font-semibold">"next tab"</code> / <code className="text-foreground font-semibold">"prev tab"</code> / <code className="text-foreground font-semibold">"close tab"</code></li>
                <li><code className="text-foreground font-semibold">"scroll down"</code> / <code className="text-foreground font-semibold">"scroll up"</code> (Page scroll)</li>
              </ul>
            </div>

            <div className="flex flex-col gap-1 rounded border border-border/60 bg-muted/40 p-2.5">
              <span className="font-bold uppercase text-foreground">⏪ Undo & Corrections</span>
              <ul className="list-disc pl-4 text-muted-foreground space-y-0.5">
                <li><code className="text-foreground font-semibold">"scratch that"</code> / <code className="text-foreground font-semibold">"undo"</code>: Reverts last text</li>
                <li><code className="text-foreground font-semibold">"delete word"</code>: Erases word before cursor</li>
                <li><code className="text-foreground font-semibold">"delete line"</code>: Clears current line</li>
                <li><code className="text-foreground font-semibold">"clear all"</code>: Empties document/input</li>
              </ul>
            </div>

            <div className="flex flex-col gap-1 rounded border border-border/60 bg-muted/40 p-2.5">
              <span className="font-bold uppercase text-foreground">🔠 Smart Case & Formatting</span>
              <ul className="list-disc pl-4 text-muted-foreground space-y-0.5">
                <li><code className="text-foreground font-semibold">"camel case &lt;phrase&gt;"</code> → camelCase</li>
                <li><code className="text-foreground font-semibold">"snake case &lt;phrase&gt;"</code> → snake_case</li>
                <li><code className="text-foreground font-semibold">"all caps &lt;phrase&gt;"</code> → ALL CAPS</li>
                <li><code className="text-foreground font-semibold">"new line"</code> / <code className="text-foreground font-semibold">"new paragraph"</code> / <code className="text-foreground font-semibold">"bullet point"</code></li>
              </ul>
            </div>

            <div className="flex flex-col gap-1 rounded border border-border/60 bg-muted/40 p-2.5">
              <span className="font-bold uppercase text-foreground">💤 Handsfree & Snippets</span>
              <ul className="list-disc pl-4 text-muted-foreground space-y-0.5">
                <li><code className="text-foreground font-semibold">"go to sleep"</code> / <code className="text-foreground font-semibold">"stop listening"</code></li>
                <li><code className="text-foreground font-semibold">"insert snippet &lt;name&gt;"</code>: Expands saved snippet</li>
                <li><code className="text-foreground font-semibold">"git status"</code> / <code className="text-foreground font-semibold">"clear terminal"</code></li>
              </ul>
            </div>
          </div>
        )}
      </div>

      {/* --- AI Voice Polish (Groq API & Local SLM) Card --- */}
      <div className="flex flex-col gap-3 rounded-none border-2 border-border bg-card p-4 shadow-[3px_3px_0_0_var(--od-shadow)]">
        <div className="flex items-center justify-between gap-2">
          <div className="flex flex-col gap-1">
            <div className="flex items-center gap-2">
              <span className="text-sm font-bold uppercase tracking-wider text-foreground">
                ✨ AI Voice Polish
              </span>
              <Badge variant={settings?.polish_provider !== "off" ? "default" : "outline"}>
                {settings?.polish_provider === "groq"
                  ? "Groq Cloud LPU ⚡"
                  : settings?.polish_provider === "local_slm"
                  ? "Offline SLM"
                  : "Off"}
              </Badge>
            </div>
            <p className="text-xs text-muted-foreground">
              Automatically eliminates stutter, verbal fillers ("um", "like", "you know"), corrects grammar, and formats text in real-time.
            </p>
          </div>

          <Select
            value={settings?.polish_provider ?? "off"}
            onValueChange={handlePolishProviderChange}
          >
            <SelectTrigger className="w-48">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="off">Off (Raw Voice)</SelectItem>
              <SelectItem value="groq">Groq LPU (Ultra-Fast ⚡)</SelectItem>
              <SelectItem value="local_slm">Local SLM (100% Offline)</SelectItem>
            </SelectContent>
          </Select>
        </div>

        {settings?.polish_provider !== "off" && (
          <div className="flex flex-col gap-3 border-t-2 border-border/40 pt-3">
            <div className="flex items-center justify-between gap-2">
              <Label htmlFor="polish-mode" className="text-xs font-bold uppercase">
                Polish Output Mode
              </Label>
              <Select
                value={settings?.polish_mode ?? "clean"}
                onValueChange={handlePolishModeChange}
              >
                <SelectTrigger id="polish-mode" className="w-44">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="clean">Clean & Grammar Polish</SelectItem>
                  <SelectItem value="bullets">Executive Bullets</SelectItem>
                </SelectContent>
              </Select>
            </div>

            {settings?.polish_provider === "groq" && (
              <div className="flex flex-col gap-2.5 rounded border border-border/60 bg-muted/20 p-3">
                <div className="flex items-center justify-between">
                  <Label htmlFor="groq-key" className="text-xs font-bold uppercase">
                    Groq Cloud API Key
                  </Label>
                  <a
                    href="https://console.groq.com/keys"
                    target="_blank"
                    rel="noreferrer"
                    className="text-[11px] text-primary underline"
                  >
                    Get free API key ↗
                  </a>
                </div>
                <div className="flex items-center gap-2">
                  <Input
                    id="groq-key"
                    type="password"
                    value={groqKeyInput}
                    onChange={(e) => setGroqKeyInput(e.target.value)}
                    placeholder="gsk_..."
                    className="text-xs font-mono"
                  />
                  <Button size="sm" onClick={handleSaveGroqKey}>
                    Save Key
                  </Button>
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={handleTestGroq}
                    disabled={testingGroq}
                  >
                    {testingGroq ? "Testing..." : "⚡ Test"}
                  </Button>
                </div>

                <div className="flex items-center justify-between gap-2 pt-1">
                  <Label htmlFor="groq-model" className="text-xs font-semibold">
                    Groq LLM Model
                  </Label>
                  <Select
                    value={settings?.groq_model ?? "llama-3.1-8b-instant"}
                    onValueChange={handleGroqModelChange}
                  >
                    <SelectTrigger id="groq-model" className="w-56">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="llama-3.1-8b-instant">
                        Llama 3.1 8B Instant (Fastest)
                      </SelectItem>
                      <SelectItem value="llama-3.3-70b-versatile">
                        Llama 3.3 70B Versatile (Quality)
                      </SelectItem>
                    </SelectContent>
                  </Select>
                </div>

                {testGroqResult && (
                  <div className="rounded bg-emerald-500/10 border border-emerald-500/30 p-2 text-xs text-emerald-600 dark:text-emerald-400">
                    <strong>Test Output:</strong> {testGroqResult}
                  </div>
                )}
              </div>
            )}
          </div>
        )}
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
