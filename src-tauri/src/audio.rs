use rodio::{buffer::SamplesBuffer, OutputStream, Sink};

/// Synthesized audio cues for dictation state. All tones are generated at
/// runtime — no bundled assets — and amplitude is scaled by the configured
/// volume. Playback runs on a detached thread and fails silently.

#[derive(Debug, Clone, Copy)]
pub enum SoundEvent {
    Listening,
    Inserted,
    Error,
}

const SAMPLE_RATE: u32 = 44_100;

fn tone(freq: f32, seconds: f32, volume: f32) -> SamplesBuffer<f32> {
    let count = (SAMPLE_RATE as f32 * seconds) as usize;
    let samples: Vec<f32> = (0..count)
        .map(|i| {
            let t = i as f32 / SAMPLE_RATE as f32;
            let phase = t * freq * 2.0 * std::f32::consts::PI;
            let envelope = (1.0 - t / seconds).max(0.0);
            phase.sin() * envelope * volume
        })
        .collect();
    SamplesBuffer::new(1, SAMPLE_RATE, samples)
}

/// A glissando from `start_freq` to `end_freq` (phase-integrated so the pitch
/// sweep is linear and continuous).
fn sweep(start_freq: f32, end_freq: f32, seconds: f32, volume: f32) -> SamplesBuffer<f32> {
    let count = (SAMPLE_RATE as f32 * seconds) as usize;
    let acceleration = (end_freq - start_freq) / (2.0 * seconds);
    let samples: Vec<f32> = (0..count)
        .map(|i| {
            let t = i as f32 / SAMPLE_RATE as f32;
            let phase = 2.0 * std::f32::consts::PI * (start_freq * t + acceleration * t * t);
            let envelope = (1.0 - t / seconds).max(0.0);
            phase.sin() * envelope * volume
        })
        .collect();
    SamplesBuffer::new(1, SAMPLE_RATE, samples)
}

fn silence(seconds: f32) -> SamplesBuffer<f32> {
    SamplesBuffer::new(1, SAMPLE_RATE, vec![0.0; (SAMPLE_RATE as f32 * seconds) as usize])
}

pub fn play_event(volume: f32, event: SoundEvent) {
    let volume = volume.clamp(0.0, 1.0);
    if volume <= 0.0 {
        return;
    }
    std::thread::spawn(move || {
        let (_stream, stream_handle) = match OutputStream::try_default() {
            Ok(output) => output,
            Err(e) => {
                log::warn!("audio feedback: no output device: {e}");
                return;
            }
        };
        let sink = match Sink::try_new(&stream_handle) {
            Ok(sink) => sink,
            Err(e) => {
                log::warn!("audio feedback: failed to create sink: {e}");
                return;
            }
        };
        match event {
            SoundEvent::Listening => {
                sink.append(tone(660.0, 0.12, volume));
                sink.append(silence(0.05));
                sink.append(tone(880.0, 0.16, volume));
            }
            SoundEvent::Inserted => sink.append(tone(1046.0, 0.2, volume)),
            SoundEvent::Error => sink.append(sweep(440.0, 220.0, 0.35, volume)),
        }
        sink.sleep_until_end();
    });
}