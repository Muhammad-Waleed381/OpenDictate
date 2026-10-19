import type { ModelInfo, ModelsStatus, Settings } from "@/lib/api";

export const settings: Settings = {
  hotkey: "Ctrl+K", mic: "mic", engine: "parakeet", language: "en",
  onboarded: true, stt_model: "parakeet-tdt-ctc-110m-int8", insert_mode: "paste",
  input_target: "text",
  heatmap_color: "green", vad_sensitivity: 0.5, continuous: false, hold_to_talk: false,
  autostart: false, spoken_punctuation: false, audio_feedback: false,
  audio_feedback_volume: 0.5, handsfree_mode: false, wake_words: "computer",
  handsfree_silence_timeout_sec: 5, voice_actions_enabled: true,
  polish_provider: "off", polish_mode: "clean",
};
export const status: ModelsStatus = {
  stt_ready: false, vad_ready: true, caption_ready: false, kws_ready: false,
  gpu_mode: "cpu", gpu_active: false, streaming_rtf_x100: 0,
};
export function model(id: string, extra: Partial<ModelInfo> = {}): ModelInfo {
  return { id, name: id, kind: "stt", engine_key: id, size_bytes: 100,
    disk_bytes: 0, installed: false, available: true, streaming: false, ...extra };
}
export function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((res, rej) => { resolve = res; reject = rej; });
  return { promise, resolve, reject };
}
