import { create } from "zustand";
import * as api from "@/lib/api";

export interface ModelProgress {
  file: string;
  received: number;
  total: number;
  speedBytesPerSec?: number;
  etaSeconds?: number;
  lastUpdated?: number;
  lastSampleBytes?: number;
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
  recordingBusy: boolean;
  recordingMode: api.RecordingMode | null;
  recordingRevision: number;
  recordingGeneration: number;
  recordingOperation: number;
  mic: string | null;
  mics: api.MicDevice[];
  modelProgress: ModelProgress[];
  modelDownloads: string[];
  cancellingModels: string[];
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
  startRecording: (mode: api.RecordingMode) => Promise<boolean>;
  stopRecording: () => Promise<api.TranscriptResult | null>;
  cancelRecording: () => Promise<void>;
  setMic: (mic: string | null) => void;
  setMics: (mics: api.MicDevice[]) => void;
  setModelProgress: (modelProgress: ModelProgress[]) => void;
  addModelProgress: (progress: ModelProgress) => void;
  removeModelProgress: (file: string) => void;
  clearModelProgress: (file?: string) => void;
  setPartial: (text: string) => void;
  refreshStats: () => Promise<void>;

  refreshModels: () => Promise<void>;
  refreshModelState: () => Promise<void>;
  downloadModel: (id: string) => Promise<void>;
  cancelModel: (id: string) => Promise<void>;
  finishModel: (id: string) => void;
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
  recordingBusy: false,
  recordingMode: null,
  recordingRevision: 0,
  recordingGeneration: 0,
  recordingOperation: 0,
  mic: null,
  mics: [],
  modelProgress: [],
  modelDownloads: [],
  cancellingModels: [],
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
  setRecording: (recording) => set((state) => ({
    recording,
    recordingMode: recording ? (state.recordingMode ?? "dictate") : null,
    recordingRevision: state.recordingRevision + 1,
    recordingGeneration: state.recordingGeneration + (recording && !state.recording ? 1 : 0),
  })),
  startRecording: async (mode) => {
    if (get().recordingBusy || get().recording) return false;
    const operation = get().recordingOperation + 1;
    const revision = get().recordingRevision;
    set({ recordingBusy: true, recordingOperation: operation, recordingMode: mode });
    try {
      await api.startRecording(mode);
      if (get().recordingOperation !== operation) return false;
      if (get().recordingRevision === revision) get().setRecording(true);
      return get().recording;
    } catch (error) {
      if (get().recordingOperation === operation && get().recordingRevision === revision) {
        set({ recordingMode: null });
      }
      throw error;
    } finally {
      if (get().recordingOperation === operation) set({ recordingBusy: false });
    }
  },
  stopRecording: async () => {
    if (get().recordingBusy || !get().recording) return null;
    const operation = get().recordingOperation + 1;
    const revision = get().recordingRevision;
    const mode = get().recordingMode;
    const generation = get().recordingGeneration;
    const lastResult = get().lastResult;
    set({ recordingBusy: true, recordingOperation: operation });
    try {
      const result = await api.stopRecording();
      if (get().recordingOperation !== operation || get().recordingGeneration !== generation) return null;
      if (get().recordingRevision === revision) get().setRecording(false);
      // A transcript event may already have published this result. Never
      // replace a newer session or a newer transcript with an IPC response.
      if (mode !== "test" && !get().recording && result?.text && get().lastResult === lastResult) {
        set({ lastResult: result });
      }
      return result;
    } finally {
      if (get().recordingOperation === operation) set({ recordingBusy: false });
    }
  },
  cancelRecording: async () => {
    const operation = get().recordingOperation + 1;
    const revision = get().recordingRevision;
    set({ recordingBusy: true, recordingOperation: operation });
    try {
      await api.cancelRecording();
      if (get().recordingOperation === operation && get().recordingRevision === revision) {
        get().setRecording(false);
      }
    } finally {
      if (get().recordingOperation === operation) set({ recordingBusy: false });
    }
  },
  setMic: (mic) => set({ mic }),
  setMics: (mics) => set({ mics }),
  setModelProgress: (modelProgress) => set({ modelProgress }),
  removeModelProgress: (file) =>
    set((state) => ({
      modelProgress: state.modelProgress.filter((p) => p.file !== file),
    })),
  clearModelProgress: (file) =>
    set((state) => ({
      modelProgress: file
        ? state.modelProgress.filter((p) => p.file !== file)
        : [],
    })),
  addModelProgress: (progress) =>
    set((state) => {
      const existing = state.modelProgress.find((p) => p.file === progress.file);
      const now = Date.now();
      // A restarted download starts from 0 again: drop the stale speed/ETA
      // from the previous attempt instead of carrying it over.
      if (existing && progress.received < existing.received) {
        const rest = state.modelProgress.filter((p) => p.file !== progress.file);
        return {
          modelProgress: [
            ...rest,
            {
              ...progress,
              lastUpdated: now,
              lastSampleBytes: progress.received,
              speedBytesPerSec: 0,
              etaSeconds: undefined,
            },
          ],
        };
      }

      let speedBytesPerSec = existing?.speedBytesPerSec;
      let etaSeconds = existing?.etaSeconds;
      let lastUpdated = existing?.lastUpdated ?? now;
      let lastSampleBytes = existing?.lastSampleBytes ?? (existing?.received ?? progress.received);

      if (existing && existing.lastUpdated && now > existing.lastUpdated) {
        const dt = (now - existing.lastUpdated) / 1000;
        if (dt >= 0.25) {
          const sampleBytes = existing.lastSampleBytes ?? existing.received;
          const db = progress.received - sampleBytes;
          if (db >= 0) {
            const currentSpeed = db / dt;
            // Smooth speed with exponential moving average (60% previous, 40% current)
            speedBytesPerSec =
              existing.speedBytesPerSec && existing.speedBytesPerSec > 0
                ? existing.speedBytesPerSec * 0.6 + currentSpeed * 0.4
                : currentSpeed;
            if (speedBytesPerSec > 0 && progress.total > progress.received) {
              etaSeconds = Math.max(1, Math.round((progress.total - progress.received) / speedBytesPerSec));
            }
          }
          lastUpdated = now;
          lastSampleBytes = progress.received;
        }
      } else {
        lastUpdated = now;
        lastSampleBytes = progress.received;
      }

      const rest = state.modelProgress.filter((p) => p.file !== progress.file);
      return {
        modelProgress: [
          ...rest,
          {
            ...progress,
            speedBytesPerSec,
            etaSeconds,
            lastUpdated,
            lastSampleBytes,
          },
        ],
      };
    }),

  setPartial: (text) => set({ partial: text }),

  refreshModels: async () => {
    const models = await api.getModelsStatus();
    set({ models });
  },

  refreshModelState: async () => {
    const [models, catalog] = await Promise.all([api.getModelsStatus(), api.getModelsCatalog()]);
    set({ models, catalog });
  },

  finishModel: (id) => set((state) => ({
    modelDownloads: state.modelDownloads.filter((file) => file !== id),
    cancellingModels: state.cancellingModels.filter((file) => file !== id),
    modelProgress: state.modelProgress.filter((progress) => progress.file !== id),
  })),

  downloadModel: async (id) => {
    if (get().modelDownloads.includes(id)) throw new Error("This model is already installing");
    set((state) => ({ modelDownloads: [...state.modelDownloads, id] }));
    try {
      // Resolves after extraction and required dependencies, not at 100% bytes.
      await api.ensureModel(id);
      await get().refreshModelState();
    } finally {
      get().finishModel(id);
      // VAD may be installed as a dependency of this model.
      get().removeModelProgress("silero_vad");
    }
  },

  cancelModel: async (id) => {
    if (get().cancellingModels.includes(id)) return;
    set((state) => ({ cancellingModels: [...state.cancellingModels, id] }));
    try {
      await api.cancelModelDownload(id);
      // Cancellation only flags the worker; its terminal event/ensureModel
      // settlement clears the busy state after disk cleanup has finished.
    } catch (error) {
      set((state) => ({ cancellingModels: state.cancellingModels.filter((file) => file !== id) }));
      throw error;
    }
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
    const recordingRevision = get().recordingRevision;
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
    set({ models, catalog, history, dictionary, snippets, mics, mic, stats,
      ...(get().recordingRevision === recordingRevision ? { recording } : {}),
    });
    set({ hydrated: true });
  },
}));
