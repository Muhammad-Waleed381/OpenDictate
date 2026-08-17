import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface ModelsStatus {
  stt_ready: boolean;
  vad_ready: boolean;
}

export interface ModelInfo {
  id: string;
  name: string;
  kind: "stt" | "vad";
  engine_key: string | null;
  size_bytes: number;
  disk_bytes: number;
  installed: boolean;
  available: boolean;
}

export interface Settings {
  hotkey: string;
  mic: string | null;
  engine: string;
  language: string;
  onboarded: boolean;
  stt_model: string;
  insert_mode: string;
}

export type SettingsPatch = Partial<
  Pick<Settings, "hotkey" | "engine" | "language" | "stt_model" | "insert_mode">
>;

export interface HistoryEntry {
  id: number;
  text: string;
  created_at: string;
  duration_ms: number;
  source: string;
}

export interface DictEntry {
  id: number;
  word: string;
  created_at: string;
}

export interface TranscriptResult {
  text: string;
  duration_ms: number;
}

export type OverlayStateValue =
  | "listening"
  | "transcribing"
  | "inserted"
  | "error"
  | "hidden";

export interface OverlayState {
  state: OverlayStateValue;
  message?: string;
}

export interface AudioLevelPayload {
  rms: number;
}

export interface ModelProgressPayload {
  file: string;
  received: number;
  total: number;
}

export interface ModelsReadyPayload {}

export interface TranscriptPayload {
  text: string;
  injected: boolean;
}

export type RecordingMode = "dictate" | "test";

export function listMics(): Promise<string[]> {
  return invoke<string[]>("list_mics");
}

export function getMic(): Promise<string | null> {
  return invoke<string | null>("get_mic");
}

export function setMic(name: string): Promise<void> {
  return invoke<void>("set_mic", { name });
}

export function getModelsStatus(): Promise<ModelsStatus> {
  return invoke<ModelsStatus>("models_status");
}

export function getModelsCatalog(): Promise<ModelInfo[]> {
  return invoke<ModelInfo[]>("models_catalog");
}

export function ensureModel(id: string): Promise<void> {
  return invoke<void>("ensure_model", { id });
}

export function removeModel(id: string): Promise<void> {
  return invoke<void>("remove_model", { id });
}

export function startRecording(mode: RecordingMode): Promise<void> {
  return invoke<void>("start_recording", { mode });
}

export function stopRecording(): Promise<TranscriptResult> {
  return invoke<TranscriptResult>("stop_recording");
}

export function cancelRecording(): Promise<void> {
  return invoke<void>("cancel_recording");
}

export function isRecording(): Promise<boolean> {
  return invoke<boolean>("is_recording");
}

export function getSettings(): Promise<Settings> {
  return invoke<Settings>("get_settings");
}

export function setSettings(settings: SettingsPatch): Promise<void> {
  return invoke<void>("set_settings", { settings });
}

export function completeOnboarding(): Promise<void> {
  return invoke<void>("complete_onboarding");
}

export function getHistory(): Promise<HistoryEntry[]> {
  return invoke<HistoryEntry[]>("get_history");
}

export function deleteHistory(id: number): Promise<void> {
  return invoke<void>("delete_history", { id });
}

export function clearHistory(): Promise<void> {
  return invoke<void>("clear_history");
}

export function getDictionary(): Promise<DictEntry[]> {
  return invoke<DictEntry[]>("get_dictionary");
}

export function addDictionaryWord(word: string): Promise<void> {
  return invoke<void>("add_dictionary_word", { word });
}

export function removeDictionaryWord(word: string): Promise<void> {
  return invoke<void>("remove_dictionary_word", { word });
}

export function pasteClipboard(text: string): Promise<void> {
  return invoke<void>("paste_clipboard", { text });
}

export function copyText(text: string): Promise<void> {
  return invoke<void>("copy_text", { text });
}

export function onOverlayState(
  cb: (payload: OverlayState) => void
): Promise<UnlistenFn> {
  return listen<OverlayState>("overlay-state", (event) => cb(event.payload));
}

export function onAudioLevel(
  cb: (payload: AudioLevelPayload) => void
): Promise<UnlistenFn> {
  return listen<AudioLevelPayload>("audio-level", (event) => cb(event.payload));
}

export function onModelProgress(
  cb: (payload: ModelProgressPayload) => void
): Promise<UnlistenFn> {
  return listen<ModelProgressPayload>("model-progress", (event) =>
    cb(event.payload)
  );
}

export function onModelsReady(
  cb: (payload: ModelsReadyPayload) => void
): Promise<UnlistenFn> {
  return listen<ModelsReadyPayload>("models-ready", (event) => cb(event.payload));
}

export function onTranscript(
  cb: (payload: TranscriptPayload) => void
): Promise<UnlistenFn> {
  return listen<TranscriptPayload>("transcript", (event) => cb(event.payload));
}

export function onHistoryUpdated(cb: () => void): Promise<UnlistenFn> {
  return listen<unknown>("history-updated", () => cb());
}