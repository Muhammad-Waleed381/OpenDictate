import puppeteer from 'puppeteer-core';
import fs from 'fs';

const CHROMIUM_PATH = '/snap/bin/chromium';
const BASE_URL = 'http://127.0.0.1:4173';

async function main() {
  console.log('Launching headless browser...');
  const browser = await puppeteer.launch({
    executablePath: CHROMIUM_PATH,
    headless: true,
    args: ['--no-sandbox', '--disable-setuid-sandbox', '--disable-gpu'],
    defaultViewport: {
      width: 1240,
      height: 800,
      deviceScaleFactor: 2,
    },
  });

  const page = await browser.newPage();

  await page.evaluateOnNewDocument(() => {
    localStorage.setItem('od:sidebar', 'collapsed');

    const defaultSettings = {
      gpu: 'auto',
      hotkey: 'Ctrl+Alt+Space',
      mic: 'Studio USB Microphone',
      engine: 'nemo_transducer',
      language: 'en',
      onboarded: true,
      stt_model: 'parakeet-tdt-110m',
      insert_mode: 'auto',
      heatmap_color: 'green',
      vad_sensitivity: 0.5,
      continuous: false,
      hold_to_talk: false,
      autostart: true,
      spoken_punctuation: true,
      audio_feedback: true,
      audio_feedback_volume: 0.5,
      handsfree_mode: false,
      wake_words: 'hey dictate',
      handsfree_silence_timeout_sec: 10,
      voice_actions_enabled: true,
      polish_provider: 'off',
      polish_mode: 'clean',
    };

    const daily = [];
    const now = new Date();
    for (let i = 60; i >= 0; i--) {
      const d = new Date(now.getTime() - i * 24 * 60 * 60 * 1000);
      const iso = d.toISOString().slice(0, 10);
      const words = (i % 7 === 0 || i % 7 === 6) ? Math.floor(Math.random() * 400 + 200) : Math.floor(Math.random() * 1800 + 800);
      daily.push({ day: iso, words });
    }

    window.__TAURI_INTERNALS__ = {
      invoke: async (cmd, args = {}) => {
        switch (cmd) {
          case 'get_settings':
            return defaultSettings;
          case 'models_status':
            return {
              stt_ready: true,
              vad_ready: true,
              caption_ready: true,
              kws_ready: true,
              gpu_mode: 'CUDA',
              gpu_active: true,
              streaming_rtf_x100: 4,
            };
          case 'models_catalog':
            return [
              {
                id: 'parakeet-tdt-110m',
                name: 'Parakeet TDT 110M (int8)',
                kind: 'stt',
                engine_key: 'parakeet-tdt-110m',
                size_bytes: 104857600,
                disk_bytes: 104857600,
                installed: true,
                available: true,
                streaming: false,
              },
              {
                id: 'nemo-streaming-fastconformer-ctc-en-80ms',
                name: 'NVIDIA NeMo FastConformer 80ms (Streaming)',
                kind: 'stt',
                engine_key: 'nemo-streaming-fastconformer-ctc-en-80ms',
                size_bytes: 115343360,
                disk_bytes: 115343360,
                installed: true,
                available: true,
                streaming: true,
              },
              {
                id: 'whisper-turbo',
                name: 'Whisper Turbo (Large v3)',
                kind: 'stt',
                engine_key: 'whisper-turbo',
                size_bytes: 591724544,
                disk_bytes: 591724544,
                installed: true,
                available: true,
                streaming: false,
              },
              {
                id: 'parakeet-tdt-0.6b-v3',
                name: 'Parakeet TDT 0.6B v3',
                kind: 'stt',
                engine_key: 'parakeet-tdt-0.6b-v3',
                size_bytes: 511705088,
                disk_bytes: 0,
                installed: false,
                available: true,
                streaming: false,
              },
              {
                id: 'zipformer-20m',
                name: 'Zipformer EN 20M (Live Captions)',
                kind: 'caption',
                engine_key: 'zipformer-20m',
                size_bytes: 30408704,
                disk_bytes: 30408704,
                installed: true,
                available: true,
                streaming: true,
              },
              {
                id: 'silero-vad-v4',
                name: 'Silero VAD v4',
                kind: 'vad',
                engine_key: 'silero-vad-v4',
                size_bytes: 1782579,
                disk_bytes: 1782579,
                installed: true,
                available: true,
                streaming: false,
              },
            ];
          case 'list_mics':
            return [
              { id: '1', label: 'Studio USB Microphone (Default)' },
              { id: '2', label: 'Built-in Audio' },
            ];
          case 'get_mic':
            return 'Studio USB Microphone';
          case 'is_recording':
            return false;
          case 'word_stats':
            return {
              daily,
              total_words: 48950,
              total_sessions: 1340,
              streak_days: 12,
              best_day: '2026-08-28',
              best_words: 3450,
            };
          case 'get_history':
            return [
              {
                id: 1,
                text: 'OpenDictate is an ultra-fast, local-first voice dictation tool with zero cloud dependency and zero subscription fees.',
                duration_ms: 3200,
                wpm: 185,
                created_at: '2026-08-30T12:30:00Z',
              },
              {
                id: 2,
                text: 'The new NVIDIA NeMo FastConformer model provides streaming live transcription with sub-100 millisecond response times.',
                duration_ms: 4100,
                wpm: 205,
                created_at: '2026-08-30T11:45:00Z',
              },
              {
                id: 3,
                text: 'Voice coding commands allow instant formatting into camel case, snake case, and capital letters directly in the editor.',
                duration_ms: 3600,
                wpm: 190,
                created_at: '2026-08-30T10:15:00Z',
              },
            ];
          case 'get_dictionary':
            return [
              { id: 1, word: 'OpenDictate', created_at: '2026-08-01' },
              { id: 2, word: 'FastConformer', created_at: '2026-08-05' },
              { id: 3, word: 'TypeScript', created_at: '2026-08-10' },
              { id: 4, word: 'Kubernetes', created_at: '2026-08-15' },
              { id: 5, word: 'Sherpa-ONNX', created_at: '2026-08-20' },
            ];
          case 'get_snippets':
          case 'list_snippets':
            return [
              {
                id: 1,
                trigger: 'signature',
                text: 'Best regards,\nMuhammad Waleed\nOpenDictate Maintainer',
                created_at: '2026-08-01',
              },
              {
                id: 2,
                trigger: 'meeting notes',
                text: '## 📝 Meeting Notes\n- **Date:** {{date}}\n- **Attendees:** \n- **Key Decisions:** \n- **Action Items:** ',
                created_at: '2026-08-10',
              },
              {
                id: 3,
                trigger: 'quick reply',
                text: 'Thanks for reaching out! I will review your PR and get back to you shortly.',
                created_at: '2026-08-15',
              },
            ];
          default:
            return null;
        }
      },
      transformCallback: (callback, once) => 1,
    };
  });

  console.log(`Navigating to ${BASE_URL}...`);
  await page.goto(BASE_URL, { waitUntil: 'networkidle0' });
  await new Promise((resolve) => setTimeout(resolve, 2000));

  async function clickTabByIndex(index, filename) {
    console.log(`Switching to tab index ${index} -> ${filename}...`);
    await page.evaluate((idx) => {
      const btns = Array.from(document.querySelectorAll('aside nav button'));
      if (btns[idx]) btns[idx].click();
    }, index);
    await new Promise((resolve) => setTimeout(resolve, 800));
    await page.screenshot({ path: `docs/screenshots/${filename}` });
  }

  // 0: Home / Dashboard
  await clickTabByIndex(0, 'dashboard.png');
  fs.copyFileSync('docs/screenshots/dashboard.png', 'revamp-main.png');

  // 5: Models Hub
  await clickTabByIndex(5, 'models.png');

  // 2: Dictionary
  await clickTabByIndex(2, 'dictionary.png');

  // 3: Snippets
  await clickTabByIndex(3, 'snippets.png');

  // 4: History
  await clickTabByIndex(4, 'history.png');

  // 1: Activity / Heatmap
  await clickTabByIndex(1, 'activity.png');

  console.log('All tabs captured cleanly!');
  await browser.close();
  process.exit(0);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
