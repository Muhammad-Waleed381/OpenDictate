//! Small policies shared by the recording workers. They deliberately have no
//! desktop or inference dependencies so failure paths can be exercised safely.
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkToken {
    pub generation: u64,
    pub handsfree_generation: Option<u64>,
}

/// Capture transitions are short and serialized. Inference runs outside that
/// lock, while cancellation/new starts invalidate its token. Irreversible work
/// is checked under the commit mutex, so cancellation cannot race a checked
/// but not-yet-executed insertion.
#[derive(Default)]
pub struct SessionControl {
    pub transition: Mutex<()>,
    pub settings_update: Mutex<()>,
    pub inference: Mutex<()>,
    generation: AtomicU64,
    handsfree_generation: AtomicU64,
    commits: Mutex<()>,
}

impl SessionControl {
    pub fn token(&self) -> WorkToken {
        WorkToken {
            generation: self.generation.load(Ordering::SeqCst),
            handsfree_generation: None,
        }
    }

    pub fn handsfree_token(&self) -> WorkToken {
        WorkToken {
            handsfree_generation: Some(self.handsfree_generation.load(Ordering::SeqCst)),
            ..self.token()
        }
    }

    pub fn invalidate(&self) -> WorkToken {
        let _guard = self.commits.lock().unwrap_or_else(|e| e.into_inner());
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.token()
    }

    pub fn restart_handsfree(&self) -> u64 {
        let _guard = self.commits.lock().unwrap_or_else(|e| e.into_inner());
        self.handsfree_generation.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn current(&self, token: WorkToken) -> bool {
        token.generation == self.generation.load(Ordering::SeqCst)
            && token
                .handsfree_generation
                .is_none_or(|g| g == self.handsfree_generation.load(Ordering::SeqCst))
    }

    pub fn handsfree_current(&self, generation: u64) -> bool {
        self.handsfree_generation.load(Ordering::SeqCst) == generation
    }

    pub fn commit_guard(&self, token: WorkToken) -> Result<MutexGuard<'_, ()>, String> {
        let guard = self.commits.lock().map_err(|e| e.to_string())?;
        if !self.current(token) {
            return Err("dictation session was cancelled or replaced".to_string());
        }
        Ok(guard)
    }
}

/// Establish a fresh explicit recording, distinct from recovery of a failed
/// capture that belongs to an existing session.
pub fn begin_explicit_capture(
    recording: bool,
    discard_previous: impl FnOnce() -> Result<(), String>,
    clear: impl FnOnce(),
    start: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    if !recording {
        // A failed backend can retain a final tail. Join/discard that producer
        // before clearing; otherwise it could append stale audio after a clear.
        discard_previous()?;
    }
    clear();
    if recording {
        Ok(())
    } else {
        start()
    }
}

/// Complete a dictated command and submit only an actual insertion.
pub fn submit_after_insert(
    insert: impl FnOnce() -> Result<bool, String>,
    submit: impl FnOnce() -> Result<(), String>,
) -> Result<bool, String> {
    let inserted = insert()?;
    if inserted {
        submit()?;
    }
    Ok(inserted)
}

/// Tracks the samples around a two-observation speech onset.
pub struct SpeechBuffer {
    samples: Vec<f32>,
    pre_roll: VecDeque<f32>,
    consecutive: usize,
    pub active: bool,
    limit: usize,
    pre_roll_limit: usize,
}

impl SpeechBuffer {
    pub fn new(limit: usize, pre_roll_limit: usize) -> Self {
        Self {
            samples: Vec::new(),
            pre_roll: VecDeque::new(),
            consecutive: 0,
            active: false,
            limit,
            pre_roll_limit,
        }
    }

    pub fn push(&mut self, samples: &[f32], speech: bool) -> Vec<Vec<f32>> {
        if !self.active {
            self.pre_roll.extend(samples.iter().copied());
            while self.pre_roll.len() > self.pre_roll_limit {
                self.pre_roll.pop_front();
            }
            self.consecutive = if speech { self.consecutive + 1 } else { 0 };
            if self.consecutive < 2 {
                return Vec::new();
            }
            self.active = true;
            self.samples.extend(self.pre_roll.drain(..));
        } else {
            self.samples.extend_from_slice(samples);
        }
        // Bound every observation, including uninterrupted speech. Splitting
        // keeps the remainder in order instead of dropping the overrun.
        let mut completed = Vec::new();
        while self.samples.len() >= self.limit {
            let remainder = self.samples.split_off(self.limit);
            completed.push(std::mem::replace(&mut self.samples, remainder));
        }
        completed
    }

    pub fn take(&mut self) -> Vec<f32> {
        self.active = false;
        self.consecutive = 0;
        self.pre_roll.clear();
        std::mem::take(&mut self.samples)
    }
}

pub fn request_download_cancel(
    downloads: &std::collections::HashMap<String, std::sync::Arc<std::sync::atomic::AtomicBool>>,
    id: &str,
) {
    if let Some(flag) = downloads.get(id) {
        flag.store(true, Ordering::SeqCst);
    }
}

pub fn release_download_owner(
    downloads: &mut std::collections::HashMap<
        String,
        std::sync::Arc<std::sync::atomic::AtomicBool>,
    >,
    id: &str,
    owner: &std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    if downloads
        .get(id)
        .is_some_and(|current| std::sync::Arc::ptr_eq(current, owner))
    {
        downloads.remove(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn explicit_start_reuses_live_handsfree_capture_without_reopening_it() {
        let cleared = Cell::new(false);
        begin_explicit_capture(
            true,
            || panic!("live capture must stay open"),
            || cleared.set(true),
            || panic!("live capture must not be restarted"),
        )
        .unwrap();
        assert!(cleared.get());
    }

    #[test]
    fn explicit_start_discards_failed_capture_audio_after_joining_its_producer() {
        let audio = std::cell::RefCell::new(vec![1.0, 2.0]);
        let events = std::cell::RefCell::new(Vec::new());
        begin_explicit_capture(
            false,
            || {
                events.borrow_mut().push("join");
                audio.borrow_mut().push(3.0);
                Ok(())
            },
            || {
                events.borrow_mut().push("clear");
                audio.borrow_mut().clear();
            },
            || {
                events.borrow_mut().push("start");
                audio.borrow_mut().push(4.0);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(
            *audio.borrow(),
            vec![4.0],
            "failed background audio must not enter the explicit session"
        );
        assert_eq!(*events.borrow(), vec!["join", "clear", "start"]);
    }

    #[test]
    fn cancellation_retains_download_ownership_until_worker_exits() {
        let owner = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut downloads = std::collections::HashMap::from([("model".to_string(), owner.clone())]);
        request_download_cancel(&downloads, "model");
        assert!(owner.load(Ordering::SeqCst));
        assert!(downloads.contains_key("model"));
        release_download_owner(&mut downloads, "model", &owner);
        assert!(!downloads.contains_key("model"));
        request_download_cancel(&downloads, "missing");
        assert!(downloads.is_empty());
    }

    #[test]
    fn stale_download_cleanup_cannot_release_a_replacement() {
        let old = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let new = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut downloads = std::collections::HashMap::from([("model".to_string(), new.clone())]);
        release_download_owner(&mut downloads, "model", &old);
        assert!(std::sync::Arc::ptr_eq(&downloads["model"], &new));
    }

    #[test]
    fn cancellation_and_new_sessions_reject_stale_commits() {
        let control = SessionControl::default();
        let old = control.token();
        let current = control.invalidate();
        assert!(control.commit_guard(old).is_err());
        assert!(control.commit_guard(current).is_ok());
    }

    #[test]
    fn cancellation_can_invalidate_while_capture_transition_waits_for_decode() {
        let control = std::sync::Arc::new(SessionControl::default());
        let token = control.token();
        let _transition = control.transition.lock().unwrap();
        let other = control.clone();
        std::thread::spawn(move || other.invalidate())
            .join()
            .unwrap();
        assert!(control.commit_guard(token).is_err());
    }

    #[test]
    fn successful_insert_submits_once_and_propagates_key_failure() {
        let submitted = Cell::new(0);
        let result = submit_after_insert(
            || Ok(true),
            || {
                submitted.set(submitted.get() + 1);
                Err("key injection failed".into())
            },
        );
        assert_eq!(submitted.get(), 1);
        assert_eq!(result.unwrap_err(), "key injection failed");
    }

    #[test]
    fn handsfree_restart_does_not_invalidate_explicit_recording() {
        let control = SessionControl::default();
        let explicit = control.token();
        let background = control.handsfree_token();
        control.restart_handsfree();
        assert!(control.current(explicit));
        assert!(!control.current(background));
    }

    #[test]
    fn resetting_onset_discards_prior_session_audio() {
        let mut buffer = SpeechBuffer::new(100, 3);
        buffer.push(&[9.0, 9.0], true);
        buffer.take();
        buffer.push(&[1.0], true);
        buffer.push(&[2.0], true);
        assert_eq!(buffer.take(), vec![1.0, 2.0]);
    }

    #[test]
    fn failed_insert_never_submits_existing_content() {
        let submitted = Cell::new(false);
        let result = submit_after_insert(
            || Err("clipboard unavailable".into()),
            || {
                submitted.set(true);
                Ok(())
            },
        );
        assert!(
            !submitted.get(),
            "Enter must not run after insertion failure"
        );
        assert!(result.is_err());
    }

    #[test]
    fn clipboard_only_does_not_submit_existing_content() {
        let submitted = Cell::new(false);
        let result = submit_after_insert(
            || Ok(false),
            || {
                submitted.set(true);
                Ok(())
            },
        );
        assert!(!submitted.get(), "copying is not insertion");
        assert!(!result.unwrap());
    }

    #[test]
    fn confirmed_speech_includes_first_positive_batch() {
        let mut buffer = SpeechBuffer::new(100, 10);
        buffer.push(&[1.0, 2.0], true);
        buffer.push(&[3.0, 4.0], true);
        assert_eq!(buffer.take(), vec![1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn continuous_speech_is_bounded_without_losing_chunk_remainder() {
        let mut buffer = SpeechBuffer::new(4, 10);
        let mut emitted = buffer.push(&[1.0, 2.0], true);
        emitted.extend(buffer.push(&[3.0, 4.0, 5.0, 6.0], true));
        assert_eq!(emitted, vec![vec![1.0, 2.0, 3.0, 4.0]]);
        assert_eq!(buffer.take(), vec![5.0, 6.0]);
    }
}
