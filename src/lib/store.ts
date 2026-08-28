import { create } from "zustand";
import * as api from "@/lib/api";

export interface ModelProgress {
  file: string;
  received: number;
  total: number;
  speedBytesPerSec?: number;
  etaSeconds?: number;
  lastUpdated?: number;
}

interface OpenDictateStore {
  level: number;
  overlayState: api.OverlayState | null;
  models: api.ModelsStatus | null;
  catalog: api.ModelInfo[];
  settings: api.Settings | null;
  history: api.HistoryEntry[];
  dictionary: api.DictEntry[];
  snippets: api.SnippetEntry[];
  recording: boolean;
  mic: string | null;
  mics: api.MicDevice[];
  modelProgress: ModelProgress[];
  lastResult: api.TranscriptResult | null;
  stats: api.WordStats | null;
  partial: string;
  settingsRevision: number;
  hydrated: boolean;

  setLevel: (level: number) => void;
  setOverlayState: (overlayState: api.OverlayState) => void;
  setModels: (models: api.ModelsStatus) => void;
  setCatalog: (catalog: api.ModelInfo[]) => void;
  setSettings: (settings: api.Settings) => void;
  setHistory: (history: api.HistoryEntry[]) => void;
  setDictionary: (dictionary: api.DictEntry[]) => void;
  setSnippets: (snippets: api.SnippetEntry[]) => void;
  setRecording: (recording: boolean) => void;
  setMic: (mic: string | null) => void;
  setMics: (mics: api.MicDevice[]) => void;
  setModelProgress: (modelProgress: ModelProgress[]) => void;
  addModelProgress: (progress: ModelProgress) => void;
  removeModelProgress: (file: string) => void;
  setPartial: (text: string) => void;
  refreshStats: () => Promise<void>;

  refreshModels: () => Promise<void>;
  refreshCatalog: () => Promise<void>;
  refreshAll: () => Promise<void>;
}

export const useStore = create<OpenDictateStore>()((set, get) => ({
  level: 0,
  overlayState: null,
  models: null,
  catalog: [],
  settings: null,
  settingsRevision: 0,
  history: [],
  dictionary: [],
  snippets: [],
  recording: false,
  mic: null,
  mics: [],
  modelProgress: [],
  lastResult: null,
  stats: null,
  partial: "",
  hydrated: false,

  setLevel: (level) => set({ level }),
  setOverlayState: (overlayState) => set({ overlayState }),
  setModels: (models) => set({ models }),
  setCatalog: (catalog) => set({ catalog }),
  setSettings: (settings) =>
    set((state) => ({ settings, settingsRevision: state.settingsRevision + 1 })),
  setHistory: (history) => set({ history }),
  setDictionary: (dictionary) => set({ dictionary }),
  setSnippets: (snippets) => set({ snippets }),
  setRecording: (recording) => set({ recording }),
  setMic: (mic) => set({ mic }),
  setMics: (mics) => set({ mics }),
  setModelProgress: (modelProgress) => set({ modelProgress }),
  removeModelProgress: (file) =>
    set((state) => ({
      modelProgress: state.modelProgress.filter((p) => p.file !== file),
    })),
  addModelProgress: (progress) =>
    set((state) => {
      const existing = state.modelProgress.find((p) => p.file === progress.file);
      const now = Date.now();
      // A restarted download starts from 0 again: drop the stale speed/ETA
      // from the previous attempt instead of carrying it over.
      if (existing && progress.received < existing.received) {
        const rest = state.modelProgress.filter((p) => p.file !== progress.file);
        return { modelProgress: [...rest, { ...progress, lastUpdated: now }] };
      }
      let speedBytesPerSec = existing?.speedBytesPerSec;
      let etaSeconds = existing?.etaSeconds;
      if (existing && existing.lastUpdated && now > existing.lastUpdated) {
        const dt = (now - existing.lastUpdated) / 1000;
        const db = progress.received - existing.received;
        if (dt >= 0.25 && db >= 0) {
          const currentSpeed = db / dt;
          speedBytesPerSec = existing.speedBytesPerSec
            ? existing.speedBytesPerSec * 0.7 + currentSpeed * 0.3
            : currentSpeed;
          if (speedBytesPerSec > 0 && progress.total > progress.received) {
            etaSeconds = Math.round((progress.total - progress.received) / speedBytesPerSec);
          }
        }
      }
      const rest = state.modelProgress.filter((p) => p.file !== progress.file);
      if (progress.total > 0 && progress.received >= progress.total) {
        return { modelProgress: rest };
      }
      return {
        modelProgress: [
          ...rest,
          {
            ...progress,
            speedBytesPerSec,
            etaSeconds,
            lastUpdated: existing?.lastUpdated && now - existing.lastUpdated < 250 ? existing.lastUpdated : now,
          },
        ],
      };
    }),

  setPartial: (text) => set({ partial: text }),

  refreshModels: async () => {
    const models = await api.getModelsStatus();
    set({ models });
  },

  refreshStats: async () => {
    const stats = await api.getWordStats();
    set({ stats });
  },

  refreshCatalog: async () => {
    const catalog = await api.getModelsCatalog();
    set({ catalog });
  },

  refreshAll: async () => {
    // Fetch settings independently and always apply it, so a failure in any
    // other call (stats, catalog, etc.) can't block the UI from reflecting
    // the real backend state.
    const revision = get().settingsRevision;
    const settings = await api.getSettings().catch(() => null);
    if (settings && get().settingsRevision === revision) set({ settings });
    const [models, catalog, history, dictionary, snippets, mics, mic, recording, stats] =
      await Promise.all([
        api.getModelsStatus().catch(() => null),
        api.getModelsCatalog().catch(() => []),
        api.getHistory().catch(() => []),
        api.getDictionary().catch(() => []),
        api.listSnippets().catch(() => []),
        api.listMics().catch(() => []),
        api.getMic().catch(() => null),
        api.isRecording().catch(() => false),
        api.getWordStats().catch(() => null),
      ]);
    set({ models, catalog, history, dictionary, snippets, mics, mic, recording, stats });
    set({ hydrated: true });
  },
}));
