import { useState } from "react";
import { useStore } from "@/lib/store";
import * as api from "@/lib/api";
import { Label } from "@/components/ui/label";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { ModelCard } from "@/components/ModelCard";

export function GeneralTab() {
  const mics = useStore((s) => s.mics);
  const mic = useStore((s) => s.mic);
  const settings = useStore((s) => s.settings);
  const [hotkey, setHotkey] = useState(settings?.hotkey ?? "");
  const [applied, setApplied] = useState(false);

  const handleMicChange = async (name: string | null) => {
    if (!name) return;
    try {
      await api.setMic(name);
      useStore.getState().setMic(name);
    } catch {}
  };

  const handleEngineChange = async (engine: string | null) => {
    if (!engine) return;
    try {
      await api.setSettings({ engine });
      useStore.getState().refreshAll();
    } catch {}
  };

  const handleLanguageChange = async (language: string | null) => {
    if (!language) return;
    try {
      await api.setSettings({ language });
      useStore.getState().refreshAll();
    } catch {}
  };

  const handleApplyHotkey = async () => {
    const trimmed = hotkey.trim().toLowerCase();
    if (!trimmed) return;
    try {
      await api.setSettings({ hotkey: trimmed });
      await useStore.getState().refreshAll();
      setApplied(true);
      setTimeout(() => setApplied(false), 1500);
    } catch {}
  };

  return (
    <div className="flex flex-col gap-6">
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
        <div className="flex flex-col gap-2">
          <Label htmlFor="mic">Microphone</Label>
          <Select value={mic ?? ""} onValueChange={handleMicChange}>
            <SelectTrigger className="w-full" id="mic">
              <SelectValue />
            </SelectTrigger>
            <SelectContent className="w-full">
              {mics.length === 0 ? (
                <SelectItem value="__none__" disabled>
                  No microphones found
                </SelectItem>
              ) : (
                mics.map((name) => (
                  <SelectItem key={name} value={name}>
                    {name}
                  </SelectItem>
                ))
              )}
            </SelectContent>
          </Select>
        </div>

        <div className="flex flex-col gap-2">
          <Label htmlFor="engine">Engine</Label>
          <Select
            value={settings?.engine ?? "parakeet"}
            onValueChange={handleEngineChange}
          >
            <SelectTrigger className="w-full" id="engine">
              <SelectValue />
            </SelectTrigger>
            <SelectContent className="w-full">
              <SelectItem value="parakeet">Parakeet</SelectItem>
              <SelectItem value="whisper" disabled>
                Whisper (coming soon)
              </SelectItem>
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
              <SelectValue />
            </SelectTrigger>
            <SelectContent className="w-full">
              <SelectItem value="auto">Auto (system default)</SelectItem>
              <SelectItem value="en" disabled>
                English
              </SelectItem>
              <SelectItem value="es" disabled>
                Spanish
              </SelectItem>
              <SelectItem value="fr" disabled>
                French
              </SelectItem>
              <SelectItem value="de" disabled>
                German
              </SelectItem>
            </SelectContent>
          </Select>
        </div>

        <div className="flex flex-col gap-2">
          <Label htmlFor="hotkey">Global hotkey</Label>
          <div className="flex items-center gap-2">
            <Input
              id="hotkey"
              value={hotkey}
              onChange={(e) => setHotkey(e.target.value)}
              placeholder="Ctrl+Alt+Space"
            />
            <Button
              onClick={handleApplyHotkey}
              disabled={!hotkey.trim()}
              className={applied ? "bg-[#10B981] hover:bg-[#10B981]" : ""}
            >
              {applied ? "Applied" : "Apply"}
            </Button>
          </div>
        </div>
      </div>

      <ModelCard />
    </div>
  );
}