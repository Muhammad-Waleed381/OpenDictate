//! Double-tap modifier shortcuts (macOS).
//!
//! `tauri-plugin-global-shortcut` registers through Carbon's
//! `RegisterEventHotKey`, which can only express modifier+key chords: it cannot
//! bind a bare modifier, and it has no notion of "tapped twice". Gestures like
//! double-Fn — what macOS itself uses for dictation — are therefore unreachable
//! through it. This watches the raw HID event stream and synthesises the
//! gesture instead.
//!
//! The tap is ListenOnly, so events are observed and never swallowed. A
//! consequence worth knowing: if the system's own double-Fn dictation is still
//! enabled, both fire. Turn it off in System Settings → Keyboard → Dictation.
//!
//! Requires Accessibility permission, the same grant text injection already
//! needs. Without it `CGEventTapCreate` returns null and we log and give up.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, Once};
use std::time::{Duration, Instant};

use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
use core_graphics::event::{
    CGEvent, CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
    CGEventType,
};
use tauri::AppHandle;

/// Longest a modifier may be held and still read as a tap rather than a hold.
/// Holding Cmd to type Cmd+C must never contribute to the gesture.
const MAX_TAP_HOLD: Duration = Duration::from_millis(300);

/// Longest gap between the two taps. Apple's own double-Fn sits near 500ms;
/// tighter than this and the gesture is hard to perform deliberately, looser
/// and unrelated modifier presses start pairing up by accident.
const MAX_TAP_GAP: Duration = Duration::from_millis(450);

/// Modifier bits that count as "something else is held". Caps lock, numeric-pad
/// and non-coalesced are excluded: they ride along on unrelated events and would
/// poison every tap.
const RELEVANT_MODS: u64 = CGEventFlags::CGEventFlagShift.bits()
    | CGEventFlags::CGEventFlagControl.bits()
    | CGEventFlags::CGEventFlagAlternate.bits()
    | CGEventFlags::CGEventFlagCommand.bits()
    | CGEventFlags::CGEventFlagSecondaryFn.bits();

/// Armed modifier bits, or 0 when no double-tap shortcut is configured. Held in
/// an atomic so the shortcut can be changed without tearing down the run loop.
static TARGET: AtomicU64 = AtomicU64::new(0);

static START: Once = Once::new();

struct TapState {
    /// When the target modifier went down, if it is currently held.
    pressed_at: Option<Instant>,
    /// Another key or modifier joined in, so this press is a chord, not a tap.
    poisoned: bool,
    /// When the previous clean tap completed.
    last_tap_at: Option<Instant>,
}

static STATE: Mutex<TapState> = Mutex::new(TapState {
    pressed_at: None,
    poisoned: false,
    last_tap_at: None,
});

/// Parses `double:<modifier>`; returns None for ordinary chord shortcuts.
pub fn parse(hotkey: &str) -> Option<CGEventFlags> {
    let rest = hotkey.trim().strip_prefix("double:")?;
    Some(match rest.to_ascii_lowercase().as_str() {
        "fn" | "globe" => CGEventFlags::CGEventFlagSecondaryFn,
        "cmd" | "command" | "super" | "meta" => CGEventFlags::CGEventFlagCommand,
        "ctrl" | "control" => CGEventFlags::CGEventFlagControl,
        "alt" | "option" => CGEventFlags::CGEventFlagAlternate,
        "shift" => CGEventFlags::CGEventFlagShift,
        _ => return None,
    })
}

/// Arms the gesture, starting the event tap on first use. Deferred rather than
/// started at launch so users who never pick a double-tap shortcut are not
/// prompted for Accessibility.
pub fn arm(app: &AppHandle, modifier: CGEventFlags) {
    TARGET.store(modifier.bits(), Ordering::SeqCst);
    reset();
    let app = app.clone();
    START.call_once(move || spawn_tap(app));
}

/// Disarms without stopping the run loop: the callback returns immediately
/// while TARGET is 0, so re-arming later costs nothing.
pub fn disarm() {
    TARGET.store(0, Ordering::SeqCst);
    reset();
}

fn reset() {
    if let Ok(mut st) = STATE.lock() {
        st.pressed_at = None;
        st.poisoned = false;
        st.last_tap_at = None;
    }
}

fn spawn_tap(app: AppHandle) {
    let spawned = std::thread::Builder::new()
        .name("opendictate-doubletap".to_string())
        .spawn(move || {
            let tap = CGEventTap::new(
                CGEventTapLocation::HID,
                CGEventTapPlacement::TailAppendEventTap,
                CGEventTapOptions::ListenOnly,
                vec![CGEventType::FlagsChanged, CGEventType::KeyDown],
                |_proxy, event_type, event| {
                    handle(&app, event_type, event);
                    None
                },
            );
            let Ok(tap) = tap else {
                log::warn!(
                    "double-tap shortcut unavailable: could not create the event tap. \
                     Grant Accessibility under System Settings → Privacy & Security → \
                     Accessibility, then restart."
                );
                return;
            };
            let Ok(source) = tap.mach_port.create_runloop_source(0) else {
                log::warn!("double-tap shortcut unavailable: run loop source failed");
                return;
            };
            let run_loop = CFRunLoop::get_current();
            unsafe { run_loop.add_source(&source, kCFRunLoopCommonModes) };
            tap.enable();
            log::info!("double-tap listener started");
            CFRunLoop::run_current();
        });
    if let Err(e) = spawned {
        log::warn!("double-tap shortcut unavailable: {e}");
    }
}

/// Runs on the event-tap run loop. It must return fast — blocking here stalls
/// keyboard input system-wide — so firing hands off to `toggle_dictation`,
/// which spawns its own worker, and the state lock is released first.
fn handle(app: &AppHandle, event_type: CGEventType, event: &CGEvent) {
    let target = CGEventFlags::from_bits_truncate(TARGET.load(Ordering::Relaxed));
    if target.is_empty() {
        return;
    }

    let mut fire = false;
    {
        let Ok(mut st) = STATE.lock() else { return };
        match event_type {
            // Any real keypress means the modifier is being used as a chord.
            CGEventType::KeyDown => {
                st.poisoned = true;
                st.last_tap_at = None;
            }
            CGEventType::FlagsChanged => {
                let flags = event.get_flags();
                let held = flags.contains(target);
                let others = (flags.bits() & RELEVANT_MODS) & !target.bits();

                if held {
                    if st.pressed_at.is_none() {
                        st.pressed_at = Some(Instant::now());
                        st.poisoned = others != 0;
                    }
                } else if let Some(started) = st.pressed_at.take() {
                    let was_tap = !st.poisoned && started.elapsed() <= MAX_TAP_HOLD;
                    st.poisoned = false;
                    if !was_tap {
                        st.last_tap_at = None;
                    } else {
                        match st.last_tap_at.take() {
                            Some(prev) if prev.elapsed() <= MAX_TAP_GAP => fire = true,
                            _ => st.last_tap_at = Some(Instant::now()),
                        }
                    }
                }
            }
            _ => {}
        }
    }

    if fire {
        log::info!("double-tap gesture fired");
        crate::hotkey::toggle_dictation(app);
    }
}
