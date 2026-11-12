// Captures the README screenshots from the built frontend with a mocked
// Tauri backend, so the images match the current UI without a real install.
//
//   npm run build
//   npx vite preview --port 4173 --strictPort &
//   node scripts/take-screenshots.mjs
//
// CHROMIUM_PATH overrides the browser (default: /snap/bin/chromium).

import puppeteer from "puppeteer-core";
import fs from "fs";

const CHROMIUM_PATH = process.env.CHROMIUM_PATH ?? "/snap/bin/chromium";
const BASE_URL = process.env.BASE_URL ?? "http://127.0.0.1:4173";
const OUT = "docs/screenshots";

const MB = 1_000_000;
// Mirrors crates/opendictate-core/src/stt/models.rs.
const CATALOG = [
  ["parakeet-tdt-ctc-110m-int8", "Parakeet TDT 110M (int8)", "stt", "parakeet", "NVIDIA", 104_337_827, true, false],
  ["streaming-zipformer-en-20m-2023-02-17", "Zipformer EN 20M (captions)", "caption", null, "Next-gen Kaldi", 29_000_000, true, false],
  ["silero-vad-v4", "Silero VAD v4", "vad", null, "Silero", 643_854, true, false],
  ["kws-zipformer-gigaspeech-3.3m-2024-01-01", "Zipformer 3.3M (Wake Word / KWS)", "kws", null, "Next-gen Kaldi", 15_800_000, true, false],
  ["parakeet-unified-en-0.6b-int8-streaming-560ms", "Parakeet Unified 0.6B (Streaming)", "stt", "parakeet-streaming", "NVIDIA", 501_360_769, false, true],
  ["nemo-streaming-fastconformer-ctc-en-80ms", "FastConformer 80ms (Streaming)", "stt", "fastconformer-streaming", "NVIDIA", 125_000_000, true, true],
  ["whisper-tiny-en", "Whisper Tiny (en)", "stt", "whisper", "OpenAI", 118_071_777, false, false],
  ["whisper-base-en", "Whisper Base (en)", "stt", "whisper", "OpenAI", 208_576_005, true, false],
  ["parakeet-tdt-0.6b-v3", "Parakeet TDT 0.6B (Multilingual)", "stt", "parakeet", "NVIDIA", 487_170_055, false, false],
  ["parakeet-unified-en-0.6b", "Parakeet Unified En 0.6B", "stt", "parakeet", "NVIDIA", 501_350_460, false, false],
  ["whisper-turbo-en", "Whisper Turbo (Large v3)", "stt", "whisper", "OpenAI", 563_790_207, false, false],
  ["whisper-small-en", "Whisper Small (en)", "stt", "whisper", "OpenAI", 635_693_775, false, false],
  ["whisper-medium-en", "Whisper Medium (en)", "stt", "whisper", "OpenAI", 1_905_872_689, false, false],
  ["omnilingual-asr-1600-languages-300m-ctc-int8", "Omnilingual ASR 300M (1600+ Languages)", "stt", "omnilingual", "Meta", 292_571_207, false, false],
  ["cohere-transcribe-14-lang-int8", "Cohere Transcribe 2B (int8)", "stt", "cohere", "Cohere", 1_699_791_751, false, false],
].map(([id, name, kind, engine_key, company, size_bytes, installed, streaming]) => ({
  id, name, kind, engine_key, company, size_bytes,
  disk_bytes: installed ? size_bytes : 0, installed, available: true, streaming,
}));

const SETTINGS = {
  gpu: "off", hotkey: "ctrl+alt+space", mic: null, engine: "parakeet", language: "auto",
  onboarded: true, stt_model: "parakeet-tdt-ctc-110m-int8", insert_mode: "auto",
  input_target: "text", heatmap_color: "#562ff7", vad_sensitivity: 0.5, continuous: false,
  hold_to_talk: false, autostart: true, spoken_punctuation: true, audio_feedback: true,
  audio_feedback_volume: 0.5, handsfree_mode: false, wake_words: "hey dictate, computer",
  handsfree_silence_timeout_sec: 30, voice_actions_enabled: true, polish_provider: "off",
  polish_mode: "clean", groq_api_key: null, groq_model: "llama-3.1-8b-instant",
  dock_position: "bottom_right", dock_visibility: "always", check_updates: true,
};

const NOW = Math.floor(Date.now() / 1000);
const HISTORY_TEXT = [
  "Let's move the stand-up to 10:30 tomorrow so the design review can run long.",
  "The new build fixes the crash on resume. I tested it on Ubuntu 24.04 and Fedora 41.",
  "Can you send me the Q3 numbers before Friday? I want to finish the deck over the weekend.",
  "Refactor the settings store so every write goes through one function and add a test for the rollback path.",
  "Thanks for the quick turnaround on the pull request. I left two small comments, otherwise it looks good to merge.",
  "Remind me to renew the domain and update the DNS records for the docs site.",
  "Summary of the call: they want offline support first, then team sharing in the second phase.",
  "Draft reply: happy to help, I'll take a look at the logs this afternoon and get back to you.",
];
const HISTORY = HISTORY_TEXT.map((text, i) => ({
  id: 200 - i, text, created_at: String(NOW - i * 3600 * 5 - 600),
  // 140 to 190 words a minute, like real speech.
  duration_ms: Math.round((text.split(/\s+/).length / (140 + ((i * 37) % 50))) * 60_000), source: "dictate",
}));

// A year of activity. The heatmap's shades step at 1, 3, 6 and 11+ words a
// day, so some light days are mixed in to show the full scale.
const DAILY = [];
const localDay = (d) =>
  `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
for (let i = 364; i >= 0; i--) {
  const d = new Date(Date.now() - i * 86_400_000);
  const weekend = d.getDay() === 0 || d.getDay() === 6;
  const r = ((i + 11) * 2654435761) % 1000 / 1000;
  if ((weekend && r < 0.7) || r < 0.12 || (i > 200 && i < 211)) continue;
  const words = r < 0.3 ? 1 + Math.round(r * 30) : Math.round(400 + r * 2200);
  DAILY.push({ day: localDay(d), words });
}
const TOTAL = DAILY.reduce((sum, d) => sum + d.words, 0);
const BEST = DAILY.reduce((a, b) => (b.words > a.words ? b : a));

const DICTIONARY = ["OpenDictate", "Kubernetes", "PostgreSQL", "TypeScript", "Tauri", "sherpa-onnx", "Waleed", "FastConformer", "Grafana", "OKRs"]
  .map((word, i) => ({ id: i + 1, word, created_at: String(NOW - i * 86_400 * 3) }));

const SNIPPETS = [
  ["sig", "Best regards,\nMuhammad Waleed"],
  ["standup", "Yesterday:\n- \nToday:\n- \nBlockers:\n- "],
  ["bug report", "Steps to reproduce:\n1. \nExpected:\nActual:\nVersion: "],
  ["thanks", "Thanks for reaching out! I'll take a look and get back to you shortly."],
].map(([trigger, text], i) => ({ id: i + 1, trigger, text, created_at: String(NOW - i * 86_400) }));

function mockBackend(data) {
  const callbacks = new Map();
  const listeners = [];
  let nextId = 1;
  const reply = (cmd, args) => {
    switch (cmd) {
      case "get_settings": return data.settings;
      case "models_status": return { stt_ready: true, vad_ready: true, caption_ready: true, kws_ready: true, gpu_mode: "cpu", gpu_active: false, streaming_rtf_x100: 25 };
      case "models_catalog": return data.catalog;
      case "list_mics": return [{ id: "default", label: "System default" }, { id: "usb", label: "Blue Yeti USB Microphone" }, { id: "builtin", label: "Built-in Audio Analog Stereo" }];
      case "get_mic": return null;
      case "is_recording": return false;
      case "word_stats": return data.stats;
      case "get_history": return data.history;
      case "get_history_before": return [];
      case "search_history": return data.history.filter((h) => h.text.toLowerCase().includes(String(args?.query ?? "").toLowerCase()));
      case "get_dictionary": return data.dictionary;
      case "list_snippets": return data.snippets;
      case "hold_to_talk_supported": return true;
      case "check_for_update": return { current_version: data.version, latest_version: data.version, update_available: false, can_install: false, download_size: null };
      case "plugin:event|listen": {
        listeners.push({ event: args.event, handler: args.handler });
        return nextId++;
      }
      case "plugin:event|unlisten": return null;
      default: return null;
    }
  };
  const label = new URLSearchParams(location.search).get("window") ?? "main";
  window.__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label }, currentWebview: { label, windowLabel: label } },
    transformCallback(cb, once) {
      const id = nextId++;
      callbacks.set(id, cb);
      return id;
    },
    unregisterCallback(id) { callbacks.delete(id); },
    convertFileSrc: (p) => p,
    invoke: async (cmd, args) => reply(cmd, args),
  };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
  window.__emit = (event, payload) => {
    for (const l of listeners) {
      if (l.event === event) callbacks.get(l.handler)?.({ event, id: 0, payload });
    }
  };
}

const pause = (ms) => new Promise((r) => setTimeout(r, ms));

async function main() {
  fs.mkdirSync(OUT, { recursive: true });
  const version = JSON.parse(fs.readFileSync("package.json", "utf8")).version;
  const data = {
    version,
    settings: SETTINGS,
    catalog: CATALOG,
    history: HISTORY,
    dictionary: DICTIONARY,
    snippets: SNIPPETS,
    stats: { daily: DAILY, total_words: TOTAL, total_sessions: Math.round(TOTAL / 42), streak_days: 23, best_day: BEST.day, best_words: BEST.words },
  };

  const browser = await puppeteer.launch({
    executablePath: CHROMIUM_PATH,
    headless: true,
    args: ["--no-sandbox", "--disable-gpu", "--hide-scrollbars"],
  });

  async function open({ theme = "light", hash = "home", width = 1180, height = 760, query = "" } = {}) {
    const page = await browser.newPage();
    await page.setViewport({ width, height, deviceScaleFactor: 2 });
    await page.evaluateOnNewDocument((theme) => {
      localStorage.setItem("od:theme-v2", theme);
      localStorage.setItem("od:sidebar", "expanded");
    }, theme);
    await page.evaluateOnNewDocument(mockBackend, data);
    await page.emulateMediaFeatures([{ name: "prefers-reduced-motion", value: "reduce" }]);
    await page.goto(`${BASE_URL}/${query}#${hash}`, { waitUntil: "networkidle0" });
    await pause(900);
    return page;
  }

  async function shot(page, file) {
    await page.screenshot({ path: `${OUT}/${file}` });
    console.log(`wrote ${OUT}/${file}`);
  }

  // Home, right after a dictation landed.
  for (const theme of ["light", "dark"]) {
    const page = await open({ theme });
    await page.evaluate(() => window.__emit("transcript", {
      text: "Thanks for the quick turnaround on the pull request. I left two small comments, otherwise it looks good to merge.",
      injected: true, duration_ms: 6400,
    }));
    await pause(700);
    await shot(page, theme === "light" ? "home.png" : "home-dark.png");
    await page.close();
  }

  for (const tab of ["history", "activity", "dictionary", "snippets", "models", "settings"]) {
    const page = await open({ hash: tab });
    await shot(page, `${tab}.png`);
    await page.close();
  }

  // The floating dock with a live caption, cropped from a small page.
  {
    const page = await open({ query: "?window=dock", width: 260, height: 60 });
    await page.evaluate(() => {
      window.dispatchEvent(new CustomEvent("opendictate:overlay-state", { detail: { state: "listening" } }));
      window.dispatchEvent(new CustomEvent("opendictate:partial", { detail: { text: "move the stand-up to ten thirty", streaming: true } }));
    });
    await pause(600);
    await page.screenshot({ path: `${OUT}/dock.png`, omitBackground: true, clip: { x: 0, y: 26, width: 260, height: 34 } });
    console.log(`wrote ${OUT}/dock.png`);
    await page.close();
  }

  await browser.close();
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
