import { create } from "zustand";
import * as api from "@/lib/api";

export interface ModelProgress {
  file: string;
  received: number;
  total: number;
}

interface OpenDictateStore {
  level: number;
  overlayState: api.OverlayState | null;
  models: api.ModelsStatus | null;
  catalog: api.ModelInfo[];
  settings: api.Settings | null;
  history: api.HistoryEntry[];
  dictionary: api.DictEntry[];
  recording: boolean;
  mic: string | null;
  mics: string[];
  modelProgress: ModelProgress[];
  lastResult: api.TranscriptResult | null;
  stats: api.WordStats | null;

  setLevel: (level: number) => void;
  setOverlayState: (overlayState: api.OverlayState) => void;
  setModels: (models: api.ModelsStatus) => void;
  setCatalog: (catalog: api.ModelInfo[]) => void;
  setSettings: (settings: api.Settings) => void;
  setHistory: (history: api.HistoryEntry[]) => void;
  setDictionary: (dictionary: api.DictEntry[]) => void;
  setRecording: (recording: boolean) => void;
  setMic: (mic: string | null) => void;
  setMics: (mics: string[]) => void;
  setModelProgress: (modelProgress: ModelProgress[]) => void;
  addModelProgress: (progress: ModelProgress) => void;
  refreshStats: () => Promise<void>;

  refreshModels: () => Promise<void>;
  refreshCatalog: () => Promise<void>;
  refreshAll: () => Promise<void>;
}

export const useStore = create<OpenDictateStore>()((set) => ({
  level: 0,
  overlayState: null,
  models: null,
  catalog: [],
  settings: null,
  history: [],
  dictionary: [],
  recording: false,
  mic: null,
  mics: [],
  modelProgress: [],
  lastResult: null,
  stats: null,

  setLevel: (level) => set({ level }),
  setOverlayState: (overlayState) => set({ overlayState }),
  setModels: (models) => set({ models }),
  setCatalog: (catalog) => set({ catalog }),
  setSettings: (settings) => set({ settings }),
  setHistory: (history) => set({ history }),
  setDictionary: (dictionary) => set({ dictionary }),
  setRecording: (recording) => set({ recording }),
  setMic: (mic) => set({ mic }),
  setMics: (mics) => set({ mics }),
  setModelProgress: (modelProgress) => set({ modelProgress }),
  addModelProgress: (progress) =>
    set((state) => {
      const rest = state.modelProgress.filter((p) => p.file !== progress.file);
      if (progress.total > 0 && progress.received >= progress.total) {
        return { modelProgress: rest };
      }
      return { modelProgress: [...rest, progress] };
    }),

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
    const [settings, models, catalog, history, dictionary, mics, mic, recording, stats] =
      await Promise.all([
        api.getSettings(),
        api.getModelsStatus(),
        api.getModelsCatalog(),
        api.getHistory(),
        api.getDictionary(),
        api.listMics(),
        api.getMic(),
        api.isRecording(),
        api.getWordStats(),
      ]);
    set({ settings, models, catalog, history, dictionary, mics, mic, recording, stats });
  },
}));