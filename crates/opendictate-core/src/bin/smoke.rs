use std::time::Duration;

use opendictate_core::audio::capture::AudioRecorder;
use opendictate_core::audio::vad::{apply_vad, compute_rms, SileroVad};
use opendictate_core::stt::engine::SttEngine;
use opendictate_core::stt::models::{ensure_models, is_vad_ready, stt_model_dir, vad_model_path};

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args: Vec<String> = std::env::args().collect();
    let record_secs: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(5);
    let countdown_secs: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(3);

    println!("OpenDictate smoke test — record {record_secs}s, transcribe locally");

    match run(record_secs, countdown_secs) {
        Ok(text) => {
            println!("\nTRANSCRIPT: {text}");
            if text.trim().is_empty() {
                std::process::exit(2);
            }
        }
        Err(e) => {
            eprintln!("FAILED: {e}");
            std::process::exit(1);
        }
    }
}

fn run(record_secs: u64, countdown_secs: u64) -> Result<String, String> {
    println!(
        "Models dir: {}",
        opendictate_core::stt::models::models_dir().display()
    );
    ensure_models().map_err(|e| format!("model setup failed: {e}"))?;

    println!("Input devices:");
    for (i, name) in AudioRecorder::list_input_devices().iter().enumerate() {
        println!("  {i}: {name}");
    }

    for remaining in (1..=countdown_secs).rev() {
        println!("Recording starts in {remaining}...");
        std::thread::sleep(Duration::from_secs(1));
    }

    let recorder = AudioRecorder::new();
    recorder.start().map_err(|e| e.to_string())?;
    println!("Recording now — speak a short sentence.");
    std::thread::sleep(Duration::from_secs(record_secs));
    let audio = recorder.stop().map_err(|e| e.to_string())?;

    let rms = compute_rms(&audio);
    let peak = audio.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    println!(
        "Captured {} samples at 16 kHz (RMS {rms:.4}, peak {peak:.4})",
        audio.len()
    );
    if audio.len() < (16000 * record_secs / 2) as usize {
        return Err(format!(
            "captured too little audio ({} samples); is the microphone working?",
            audio.len()
        ));
    }

    let silero = if is_vad_ready() {
        Some(SileroVad::new(&vad_model_path()).map_err(|e| e.to_string())?)
    } else {
        None
    };
    let vad_result = apply_vad(&audio, silero.as_ref());
    if !vad_result.has_speech {
        return Err(format!(
            "no speech detected (RMS {rms:.4}); speak during the recording window"
        ));
    }
    println!(
        "VAD: {} ms of speech ({} samples)",
        vad_result.speech_duration_ms,
        vad_result.trimmed_audio.len()
    );

    let engine = SttEngine::new(&stt_model_dir(), false).map_err(|e| e.to_string())?;
    let started = std::time::Instant::now();
    let text = engine
        .transcribe(&vad_result.trimmed_audio)
        .map_err(|e| e.to_string())?;
    println!("Transcribed in {:?}", started.elapsed());

    Ok(text)
}
