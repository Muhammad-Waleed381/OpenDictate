# Final Bug Arbiter report

Audit date: 2026-10-19. Role 3 follows the [Finder](finder.md) and [Skeptic](skeptic.md), before production fixes. Decisions use the checked source paths and dependency evidence, not access to hidden ground truth. A confirmed code path is distinguished from a hardware reproduction. Severity describes the supported impact; conditional platform/timing triggers remain explicit.

## Recording, recognition and session ownership

### B001 — Microphone test side effects
- **Finder:** Test speech can execute actions, snippets and cloud polish before the test guard.
- **Skeptic:** ACCEPT; the test flag exists but is checked too late. Missing-model failure is a separate symptom.
- **Analysis:** `process_utterance` in `dictation.rs:769–801` calls the action dispatcher before returning for tests; ClearAll and submit branches can affect another application. Test stop also loads STT unconditionally after VAD finds speech. The UI's mic-only setup test needs an isolated result path.
- **VERDICT: REAL BUG** — **Critical**. **Confidence: High**.

### B002 — Streaming final audio loss
- **Finder:** Stop lacks end-of-input finalization and loses audio around watermark/stop races.
- **Skeptic:** ACCEPT; draining currently ready work neither ends input nor consumes a batch copied under a previous lock.
- **Analysis:** `dictation.rs:476–502,645–678` advances the watermark before accepting a batch, snapshots before stopping capture, and only drains. The pinned Unified implementation explicitly makes its final short chunk ready after InputFinished. Both finalization and ownership of the last batch need correction.
- **VERDICT: REAL BUG** — **Medium**. **Confidence: High**.

### B003 — Inconsistent final transcript processing
- **Finder:** Streaming manual stop, streaming endpoint and offline completion apply different actions, polish and dictionary cleanup.
- **Skeptic:** ACCEPT; recognition accuracy cannot explain downstream command behavior changing with stop timing.
- **Analysis:** The three branches at `dictation.rs:522–545,678–703,775–801` differ exactly as reported. A recognized action can be executed or pasted literally depending on the endpoint. Apply one deliberate final-transcript policy while preserving test isolation and session cancellation.
- **VERDICT: REAL BUG** — **Medium**. **Confidence: High**.

### B004 — Unreachable streaming caption fallback
- **Finder:** Partial emission checks readiness immediately after draining it to false.
- **Skeptic:** ACCEPT; no waveform can be accepted between those checks while the pipe lock is held.
- **Analysis:** `dictation.rs:498–520` and `streaming.rs:248–254` establish the invariant. Reading the current result must not require another decode-ready chunk. An installed auxiliary caption engine masks this defective fallback.
- **VERDICT: REAL BUG** — **Medium**. **Confidence: High**.

### B005 — Stale caption workers
- **Finder:** A continuous caption worker survives stop and can consume or clear a later session.
- **Skeptic:** ACCEPT; captured continuous state and global cleanup lack session identity.
- **Analysis:** `spawn_caption_loop` captures `continuous` once, while ordinary `stop` never clears caption_active. A worker captured true can run indefinitely, and cleanup unconditionally removes the shared pipe. New sessions therefore need their own identity/cancellation ownership.
- **VERDICT: REAL BUG** — **Medium**. **Confidence: High**.

### B006 — Unserialized session operations and stale commits
- **Finder:** UI/hotkey operations race, and cancellation cannot invalidate in-flight inference.
- **Skeptic:** ACCEPT; hotkey-only locking and per-hook busy flags leave other callers uncovered.
- **Analysis:** `commands.rs:240–267` starts independent workers; `dictation.rs:35–55,127–190` can replace capture and later commit old results. Mutex exclusion alone would still require an explicit cancellation/session identity at irreversible action/history/insertion boundaries.
- **VERDICT: REAL BUG** — **Medium**. **Confidence: High**.

### B007 — Warmup mutates active recording state
- **Finder:** Selecting a streaming model starts a real capture consumer.
- **Skeptic:** ACCEPT; cache warmup and session creation are conflated.
- **Analysis:** `warmup_model` calls `spawn_streaming`, which replaces `state.stream`, sets its active flag and launches a consumer (`dictation.rs:381–454`). Those side effects are observable during recording and can race stale cleanup even while idle. Warmup must only prepare reusable engine state.
- **VERDICT: REAL BUG** — **Medium**. **Confidence: High**.

### B008 — Handsfree and explicit sessions process the same audio
- **Finder:** Background handsfree continues action recognition during normal recording or a mic test.
- **Skeptic:** ACCEPT; keeping the shared mic open does not transfer processing ownership.
- **Analysis:** `start` clears the buffer but the handsfree loop never excludes `user_dictation_active` or test mode. Both consumers can read subsequent samples and commit. Pause/reset background utterance and wake state at an explicit session boundary, without treating hardware ownership as user recording state.
- **VERDICT: REAL BUG** — **Medium**. **Confidence: High**.

### B009 — Failed handsfree enable wedges state
- **Finder:** Persisted success and early active=true survive failed microphone startup.
- **Skeptic:** ACCEPT; preference persistence alone cannot justify an active flag with no worker.
- **Analysis:** `start_handsfree` sets active before fallible capture/thread startup. Later attempts return early, and `set_settings` discards the first error. Runtime state and persisted intent need an explicit successful transition or rollback and a recoverable retry path.
- **VERDICT: REAL BUG** — **Medium**. **Confidence: High**.

### B010 — Running handsfree ignores changed configuration
- **Finder:** Changed wake words and microphone selection do not update the live service.
- **Skeptic:** ACCEPT; dropping a cache does not replace the worker's retained Arc.
- **Analysis:** `dictation.rs:1407,1429–1433,1489,1497` keeps the old spotter for all future keyword sessions. Settings invalidation and mic persistence cannot update it. Restart/reconfigure the actual service using a settings snapshot, retaining safe explicit-session ownership. The supplemental inverse KWS lock order is a hazard, not an independently proven reachable deadlock.
- **VERDICT: REAL BUG** — **Medium**. **Confidence: High**.

### B011 — Handsfree duration limit is conditional on silence
- **Finder:** Continuous speech can grow the utterance vector beyond its duration cap.
- **Skeptic:** ACCEPT; the bounded recorder buffer does not bound this independent vector.
- **Analysis:** `dictation.rs:1547–1576` checks max duration only in the non-speech branch. Positive VAD observations keep appending indefinitely. The elapsed-duration bound must run for both speech and silence, while preserving every chunk for the next utterance.
- **VERDICT: REAL BUG** — **Medium**. **Confidence: High**.

### B012 — Continuous offline capture has processing gaps
- **Finder:** Speech during inference/polish is never captured.
- **Skeptic:** ACCEPT with product-expectation caveat; processing is indicated but continuous listening stays enabled.
- **Analysis:** `spawn_continuous_loop` explicitly stops capture, waits synchronously for transcription/polish, then restarts. Its own contract says the mic remains open. Resumed speech after an endpoint is lost for an unbounded fraction of a normal session. Continuous capture must preserve pending audio while work is processed, with a bounded backlog and explicit overflow handling.
- **VERDICT: REAL BUG** — **Medium**. **Confidence: High**.

### B013 — Continuous meter worker can exit permanently
- **Finder:** The meter stops after the first continuous utterance.
- **Skeptic:** ACCEPT, narrowing “always” to a stop gap that overlaps a 33ms poll.
- **Analysis:** `spawn_level_emitter` exits on recorder idle and `restart_recorder` launches no replacement. Any sufficiently long inference gap exposes this; unusually fast restart can hide it. The meter should follow the logical session lifecycle.
- **VERDICT: REAL BUG** — **Low**. **Confidence: High**.

### B014 — Clearing capture invalidates watermarks
- **Finder:** Clear followed by append before polling silently skips new samples.
- **Skeptic:** ACCEPT; the existing empty-poll test covers a different interleaving.
- **Analysis:** `SharedBuffer::clear` resets base to zero after advancing it. With watermark 4 and two newly appended samples, `take_since` clamps the start to 2 and returns empty. Keep offsets monotonic, and deliberately establish the beginning of a new logical session.
- **VERDICT: REAL BUG** — **Medium**. **Confidence: High**.

### B015 — Long manual offline recording loses its beginning
- **Finder:** The rolling 120-second buffer truncates manual dictation without a guard.
- **Skeptic:** ACCEPT; a bound appropriate for streaming is not a complete offline recording.
- **Analysis:** `capture.rs:48–59` drains early samples, while ordinary offline stop transcribes only retained audio. The continuous 110-second guard is not used for manual sessions. Preserve the complete supported manual recording or enforce a visible, safe duration boundary before loss.
- **VERDICT: REAL BUG** — **Medium**. **Confidence: High**.

### B016 — CPAL capture unconditionally requests f32
- **Finder:** Integer-default microphones may fail to open.
- **Skeptic:** ACCEPT only for devices that cannot support the requested f32 configuration.
- **Analysis:** `capture.rs:298–383` copies default rate/channels but always builds a typed f32 stream. An integer default alone is not proof of failure; an f32-unsupported device is sufficient. Select/convert supported sample formats and retain the current resampling/channel policy.
- **VERDICT: REAL BUG** — **Medium**. **Confidence: High** for the compatibility defect; affected hardware was not exercised.

### B017 — PulseAudio stop/setup can block indefinitely
- **Finder:** Blocking native waits defeat cancellation and nominal deadlines.
- **Skeptic:** ACCEPT; setting an atomic does not signal a condition wait.
- **Analysis:** `pulse.rs:257,341,390–400` waits without a deadline/wakeup tied to stop; shutdown and timeout paths then join the same thread. Enumeration also blocks inside its deadline loop. A stalled source/server can wedge recording operations or the UI. Use genuinely cancellable/bounded waits, with native resource teardown after callbacks can no longer run.
- **VERDICT: REAL BUG** — **Critical**. **Confidence: High**.

### B018 — Capture failure leaves logical recording active
- **Finder:** Asynchronous capture errors do not clear the running state or notify callers.
- **Skeptic:** ACCEPT; manual retry is possible only after the user discovers and clears the stale state.
- **Analysis:** CPAL only sets stop_signal; Pulse returns from capture_loop. Neither updates `AudioRecorder.state`, so is_recording and handsfree recovery observe a dead live session. Make failure a first-class state transition, preserving safe cleanup and a user-visible error.
- **VERDICT: REAL BUG** — **Medium**. **Confidence: High**.

### B019 — Unsupported dictionary hotword configuration can terminate the app
- **Finder:** Greedy decoder configuration does not provide advertised dictionary recognition bias.
- **Skeptic:** ACCEPT and escalate: selected CTC/Unified streaming families inherit a fatal hotword stream overload.
- **Analysis:** `streaming.rs:206–210` calls the hotword overload for any nonempty dictionary. In pinned Sherpa 1.13.4, [the base overload](https://github.com/k2-fsa/sherpa-onnx/blob/v1.13.4/sherpa-onnx/csrc/online-recognizer-impl.h#L33) terminates through [the exit macro](https://github.com/k2-fsa/sherpa-onnx/blob/v1.13.4/sherpa-onnx/csrc/macros.h#L59). Neither [CTC](https://github.com/k2-fsa/sherpa-onnx/blob/v1.13.4/sherpa-onnx/csrc/online-recognizer-ctc-impl.h#L106) nor [Unified streaming](https://github.com/k2-fsa/sherpa-onnx/blob/v1.13.4/sherpa-onnx/csrc/online-recognizer-transducer-nemo-parakeet-unified-impl.h#L71) overrides it. The process-exit path was inspected, not executed. Offline NeMo supports beam-search biasing, but current greedy configuration and raw-word encoding are inadequate.
- **VERDICT: REAL BUG** — **Critical** (upgraded from Medium). **Confidence: High**.
- **Fix constraint:** Gate native biasing by the exact supported model family/decoder/tokenizer assets. Use plain streams plus dictionary text correction for unsupported CTC/Unified families; do not blanket-switch every recognizer to beam search or attempt to catch native `_Exit` as a Rust panic. Enable offline biasing only with validated encoding/configuration and test it independently.

## Text processing, actions and injection

### B020 — Dictionary punctuation/casing corruption
- **Finder:** Dictionary replacement duplicates punctuation and later cleanup changes canonical capitalization.
- **Skeptic:** ACCEPT; the replacement span covers C while inserting all of C++.
- **Analysis:** `text.rs:12–106` excludes +/# from token spans but preserves them outside the full canonical replacement, producing C++++/C##. `inject::clean_text` then capitalizes a leading iPhone. Punctuation-aware spans and applying canonical casing after generic cleanup address distinct symptoms of the same dictionary contract.
- **VERDICT: REAL BUG** — **Medium**. **Confidence: High**.

### B021 — Annotation filter removes normal content
- **Finder:** All bracketed text and substring-matched parentheticals can be deleted.
- **Skeptic:** ACCEPT; intended Whisper noise cleanup does not justify broad ordinary-text deletion.
- **Analysis:** `strip_sound_effects` explicitly accepts every square-bracket span and substrings such as ding/wind. Array indexes, coding and Windows therefore match; `clean_text` applies this to normal transcripts. Limit removal to a well-defined recognized annotation grammar; preserve ambiguous text.
- **VERDICT: REAL BUG** — **Medium**. **Confidence: High**.

### B022 — Fuzzy match scores exceed their range
- **Finder:** Repeated bigrams can outrank an exact snippet trigger.
- **Skeptic:** ACCEPT; per-pair exact handling does not stop another candidate scoring above one.
- **Analysis:** `char_dice` counts each repeated left bigram whenever any right occurrence exists. For aaaaa versus aa it returns 1.6, causing negative DP substitution cost and selection of aa over an exact candidate. Count multiset intersections correctly and preserve deterministic exact-match precedence.
- **VERDICT: REAL BUG** — **Medium**. **Confidence: High**.

### B023 — Command matching rewrites payload data
- **Finder:** URLs, prompts and snippet tails lose case and meaningful punctuation.
- **Skeptic:** ACCEPT; matching command keywords case-insensitively should not normalize arbitrary arguments.
- **Analysis:** `actions.rs:125–230` derives every payload from the already lowercased, punctuation-trimmed string. Case-sensitive URL paths are a concrete failing input. Parse command boundaries against normalized text but slice payload from the original text with Unicode-safe boundaries and explicit punctuation policy.
- **VERDICT: REAL BUG** — **Medium**. **Confidence: High**.

### B024 — Truncated polish replaces complete text
- **Finder:** Nonempty length-limited responses are accepted as complete.
- **Skeptic:** ACCEPT; a 120-second audio limit does not bound output to 1024 tokens across languages.
- **Analysis:** `polish.rs:81–139` limits tokens and discards finish_reason while accepting any nonempty content. A truncated result can silently replace the complete original. Validate completion status and preserve the original on incomplete output; a paid API call is unnecessary to test response parsing.
- **VERDICT: REAL BUG** — **Medium**. **Confidence: High**.

### B025 — Unimplemented Local SLM is selectable
- **Finder:** Settings advertises a polish provider whose implementation returns unchanged input.
- **Skeptic:** ACCEPT; an explicitly unavailable stub would have been different.
- **Analysis:** Settings offers Local SLM under an active cleanup/bullet feature, but `polish_text` always returns trimmed input. Disable/mark that provider unavailable and handle previously saved values honestly unless a real implementation is supplied. Inventing an unvalidated model integration is unnecessary for this fix.
- **VERDICT: REAL BUG** — **Medium**. **Confidence: High**.

### B026 — Recognized text crosses a script boundary
- **Finder:** App names and Windows URLs are embedded in interpreter input.
- **Skeptic:** ACCEPT for macOS/Windows paths; no remote exploit or complete speech exploit chain was demonstrated.
- **Analysis:** `inject.rs:856–875,922–927` constructs AppleScript and expandable PowerShell source from app_name and invokes cmd for a free-form URL. Argument separation in Linux does not protect these branches. Quotes/subexpressions can become code; use native/process argument APIs or literal parameters, with platform-specific regression coverage. Critical concerns the execution sink, not a claimed remote ingress.
- **VERDICT: REAL BUG** — **Critical**. **Confidence: High** for the unsafe boundary; exploit delivery through recognition was not exercised.

### B027 — Failed insertion can still submit existing content
- **Finder:** Errors are discarded before Enter and success is emitted before paste.
- **Skeptic:** ACCEPT; clipboard failure can coexist with working key injection.
- **Analysis:** PromptAndSend/TerminalCommand unconditionally send Enter after `commit_text`; other actions discard key errors. `commit_text` also emits injected=true before the fallible operation. Require a successful actual insertion before auto-submit, distinguish clipboard-only success, and publish truthful completion/error events.
- **VERDICT: REAL BUG** — **Critical**. **Confidence: High**.

### B028 — Clipboard failure returns success
- **Finder:** Linux pastes stale contents after all clipboard writers fail.
- **Skeptic:** ACCEPT; fallback attempts do not justify unconditional Ok in the all-failed branch.
- **Analysis:** `copy_to_system_clipboard` tracks every result then returns Ok even when none succeeded. `inject_text` consequently continues to paste. Propagate failure before any key injection; this is independent of callers ignoring a genuine error in B027.
- **VERDICT: REAL BUG** — **Medium**. **Confidence: High**.

### B029 — Fixed key bindings violate advertised target actions
- **Finder:** Terminal paste and delete-line shortcuts fail in common applications.
- **Skeptic:** ACCEPT with narrower scope; universal target coverage cannot be assumed.
- **Analysis:** Linux terminal commands are exposed but use Ctrl+V, whereas common terminal paste is Ctrl+Shift+V; Windows delete-line sends the same Ctrl+Backspace as delete-word. The Linux Ctrl+U effect varies by target, so do not claim universal failure. Provide a supported target/profile choice or verified target-aware mapping; changing one global shortcut to another merely moves the breakage.
- **VERDICT: REAL BUG** — **Medium**. **Confidence: High** for the specific common-target mismatches.

### B030 — Malformed wtype combinations
- **Finder:** Several fallbacks release a letter as if it were a modifier instead of typing it.
- **Skeptic:** ACCEPT; correct primary backends can hide broken fallback arrays.
- **Analysis:** `inject.rs:373,384,422,429,445` uses `-m z/u/a/c`, whereas -m releases a modifier and -k types the key. Correct complete press/key/release sequences across affected fallbacks and verify argument generation without sending real destructive keys.
- **VERDICT: REAL BUG** — **Medium**. **Confidence: High**.

### B031 — Main-window Insert/Undo targets its own focus
- **Finder:** Clicking app controls sends global keys into OpenDictate rather than the external target.
- **Skeptic:** ACCEPT; the dock's nonfocusable setting does not apply to main-window controls.
- **Analysis:** `commands.rs:700–724` immediately injects/undoes with no saved target or handoff. Main is an ordinary focusable window, so its buttons cannot assume the prior external app remains focused. The missing target transition is concrete; exact restoration behavior is platform-dependent.
- **VERDICT: REAL BUG** — **Medium**. **Confidence: High**.
- **Fix constraint:** Preserve functionality through explicit target selection/handoff: restore a known external target where supported, or hide/minimize and clearly let the user choose the target. Refuse injection while any OpenDictate window is focused and fail clearly if a safe target cannot be established. A delay alone is not target verification. Do not consume Undo state on refusal/failure. Native focus checks are still required on supported platforms.

## Model storage and download lifecycle

### B032 — Arbitrary model removal paths
- **Finder:** Unvalidated renderer input can recursively delete outside model storage.
- **Skeptic:** ACCEPT; safe normal UI IDs do not validate the command boundary.
- **Analysis:** `models.rs:292–297,1080–1093` joins arbitrary IDs, allowing absolute paths and parent traversal, then deletes recursively. Renderer-accessible remove/cancel reach it. Validate catalog membership and path containment before all destructive operations; test only temporary fixtures. No independent remote compromise is asserted.
- **VERDICT: REAL BUG** — **Critical**. **Confidence: High**.

### B033 — Cancel/retry loses download ownership
- **Finder:** An old cancelled worker can remove a replacement's map entry/files.
- **Skeptic:** ACCEPT; cancellation removes admission ownership before the worker exits.
- **Analysis:** `commands.rs:76–167` immediately deletes the entry/files on cancel and later performs unconditional worker cleanup. A retry can reuse both identity and paths in between. Retain ownership until acknowledged completion or compare a generation token, and separate temporary artifacts per install owner.
- **VERDICT: REAL BUG** — **Medium**. **Confidence: High**.

### B034 — Background installs bypass deduplication
- **Finder:** Startup/caption/VAD workers can concurrently modify shared model destinations.
- **Skeptic:** ACCEPT; only duplicate manual requests share the command-level map.
- **Analysis:** `lib.rs:154–165`, dictation auto-ensure and automatic VAD installs call core installation outside that map. Core archive, part and extraction paths are deterministic per model. Coordinate installs in the common core layer and recheck readiness after obtaining ownership; cancel/retry ownership remains a separate issue.
- **VERDICT: REAL BUG** — **Medium**. **Confidence: High**.

### B035 — Download read cannot promptly observe cancellation
- **Finder:** A stalled response/body can block indefinitely despite cancel.
- **Skeptic:** ACCEPT; connect timeout is not a response/body deadline, though eventual OS failure is possible.
- **Analysis:** `models.rs:524–584` sets only connect timeout and checks cancellation outside blocking call/read. Installed ureq defaults inspected in the preceding role provide no remaining deadline. Bound response/body inactivity or provide active cancellation; avoid a total-size-independent deadline that rejects healthy large downloads.
- **VERDICT: REAL BUG** — **Medium**. **Confidence: High**.

### B036 — KWS readiness accepts empty files
- **Finder:** Filename fallback bypasses failed size checks.
- **Skeptic:** ACCEPT; versioned-file compatibility still needs file validity.
- **Analysis:** `is_kws_ready` ORs each size-validated canonical file with a matching filename-only directory scan, which matches the same zero-byte file. Validate each fallback candidate's file type and minimum size; readiness and loader selection should agree.
- **VERDICT: REAL BUG** — **Medium**. **Confidence: High**.

### B037 — Installation completion is confused with download launch/bytes
- **Finder:** UI prematurely clears progress, leaves stale catalog/pending state, and declares KWS ready before completion.
- **Skeptic:** ACCEPT; onboarding refresh/remount can conceal the normal lifecycle failure.
- **Analysis:** Store removes byte-complete progress, App models-ready refreshes only status, and Settings awaits a command that merely spawns work. Extraction/dependencies remain active, while cached installs may never emit the intermediate progress needed to clear startingDownloads. Track explicit per-model terminal lifecycle events through extraction and refresh catalog/status together.
- **VERDICT: REAL BUG** — **Medium**. **Confidence: High**.

### B038 — Download All reads stale pending state
- **Finder:** Its wait loop advances before connection/extraction finishes; Cancel All allegedly misses connecting work.
- **Skeptic:** ACCEPT root cause, DISPROVE Cancel All subclaim because it does iterate startingDownloads.
- **Analysis:** `ModelCard.tsx:165–204` closes over the pre-update set while polling current progress. The first 500ms tick can treat a connecting model as complete. Base sequencing on explicit completion/error acknowledgement, not absent progress. No separate Cancel All omission is confirmed.
- **VERDICT: REAL BUG** — **Medium**. **Confidence: High**.

### B039 — Onboarding does not ensure the selected STT is usable
- **Finder:** Generic readiness advances the model step after an unrelated/nonselected download.
- **Skeptic:** ACCEPT; silent startup caption download itself emits no models-ready event.
- **Analysis:** `Onboarding.tsx:27–31` advances on any emitted completion, and stt_ready means any STT is installed. Download and Use are separate, so choosing a nondefault download can advance while the selected default remains absent. Gate advancement on the selected installed model and explicit selection; retain Skip as an intentional bypass.
- **VERDICT: REAL BUG** — **Medium**. **Confidence: High**.

### B040 — One CPU benchmark disables all streaming choices
- **Finder:** Global Parakeet CPU timing hides Use for unrelated/GPU models.
- **Skeptic:** ACCEPT; a useful performance warning cannot establish other model/provider performance.
- **Analysis:** `lib.rs:170–178` benchmarks only Parakeet; `ModelCard.tsx:438–451` applies the scalar to every streaming card and removes its action. Scope results to model/provider or use an advisory warning that does not block unmeasured choices.
- **VERDICT: REAL BUG** — **Medium**. **Confidence: High**.

## Database, settings and frontend state

### B041 — CSV column order is wrong
- **Finder:** Text and timestamp values are swapped relative to their headers.
- **Skeptic:** ACCEPT; substring tests do not validate schema mapping.
- **Analysis:** `commands.rs:829–842` writes id,created_at,text under id,text,created_at. This deterministic serialization defect breaks consumers. Test parsed header-to-value associations, including escaped text.
- **VERDICT: REAL BUG** — **Medium**. **Confidence: High**.

### B042 — History export drops older rows
- **Finder:** Export silently includes only 500 entries.
- **Skeptic:** ACCEPT for JSON/CSV; no text export is implemented.
- **Analysis:** `export_history` calls `db::get_history`, whose query has LIMIT 500 although older database rows remain. A display limit is not an adequate full-export query. Use an unbounded/paginated full export without changing the display limit merely to mask it.
- **VERDICT: REAL BUG** — **Medium**. **Confidence: High**.

### B043 — Pre-reset history mutates post-reset aggregates
- **Finder:** Editing/deleting an excluded old row subtracts from newly accumulated statistics.
- **Skeptic:** ACCEPT; clamping at zero prevents negatives but does not track contribution.
- **Analysis:** `db.rs:138–183,288–299` resets aggregates without marking eligible history, then applies full old-row deltas to the same date. Old 100-word row, reset, new three-word row, delete old row deterministically erases the new contribution. Track a durable contribution/reset boundary and preserve it through migration, edits and deletes.
- **VERDICT: REAL BUG** — **Medium**. **Confidence: High**.

### B044 — Statistics count spaces instead of words
- **Finder:** Repeated spaces/newlines/whitespace-only text are miscounted.
- **Skeptic:** ACCEPT; ordinary snippets bypass history, but history edits and polish remain reachable sources.
- **Analysis:** SQL counts literal spaces plus one; Rust matches that heuristic. Internal agreement does not make one-newline-two-words or repeated spaces accurate. Share whitespace-aware counting across inserts, deltas and migration/backfill without resurrecting reset data.
- **VERDICT: REAL BUG** — **Low**. **Confidence: High**.

### B045 — Zero-word today row breaks yesterday's streak
- **Finder:** Deleting today's last entry resets a still-active prior-day streak.
- **Skeptic:** ACCEPT; absent today and zero today represent equivalent current activity but take different branches.
- **Analysis:** `commands.rs:749–766` only falls back to yesterday if today's row is absent. Delete leaves a zero-valued row. Use today's positive activity, rather than row presence, to choose the streak start.
- **VERDICT: REAL BUG** — **Low**. **Confidence: High**.

### B046 — Reset Settings omits runtime/OS transitions
- **Finder:** Displayed defaults can coexist with old handsfree/autostart/provider behavior.
- **Skeptic:** ACCEPT; reset already applies some changes immediately and is not next-launch-only.
- **Analysis:** `reset_settings` writes defaults and changes hotkey/dock, but skips handsfree/autostart/cache transitions performed by normal save and discards hotkey error. Reuse an explicit settings-application path with truthful failure behavior; do not report reset success for unapplied runtime defaults.
- **VERDICT: REAL BUG** — **Medium**. **Confidence: High**.

### B047 — Hydration confuses hardware capture with a user recording
- **Finder:** Sleeping handsfree makes the main UI show an explicit recording and Stop action.
- **Skeptic:** ACCEPT; hardware state is valid for a different question than the UI asks.
- **Analysis:** `is_recording` returns recorder state, while recording-changed uses user_dictation_active. `refreshAll` overwrites that UI field with hardware state. Return the same logical-session definition used by events, with capture/processing status represented separately if needed.
- **VERDICT: REAL BUG** — **Medium**. **Confidence: High**.

### B048 — Dock lacks its event capability
- **Finder:** Capabilities list overlay but the actual window is dock, breaking recording/audio events and stop state.
- **Skeptic:** ACCEPT; local custom-command exceptions do not bypass core event plugin ACLs.
- **Analysis:** Checked installed Tauri 2.11.5: `webview/mod.rs:1823` rejects plugin calls without resolved ACL and `ipc/authority.rs:439–472` matches window/webview labels. The sole capability lacks dock. DOM fallback forwards only overlay/partial, leaving listening=true, recording=false and canStop=false. Grant the required permissions to the actual label and verify native listen/start/stop integration.
- **VERDICT: REAL BUG** — **Medium**. **Confidence: High**.

### B049 — Dock flash cleanup cancels its only expiry
- **Finder:** An early state transition can leave success/error flash indefinitely.
- **Skeptic:** ACCEPT; transitions before expiry cancel the timer without clearing state or replacing it.
- **Analysis:** `DockButton.tsx:26–38` clears the timer on every state effect cleanup but does nothing for new ordinary states. Flash rendering has priority over recording. Either clear on those transitions or give the flash its own correctly scoped lifetime.
- **VERDICT: REAL BUG** — **Low**. **Confidence: High**.

### B050 — Undo status survives a new result
- **Finder:** After one Undo, later last results remain disabled.
- **Skeptic:** ACCEPT; tab remount only hides the missing reset.
- **Analysis:** `LastResultPanel` stores undone locally and never associates it with lastResult identity. Incoming results rerender the same component. Reset on a distinct result generation, including repeated identical text; do not mark undone on failure.
- **VERDICT: REAL BUG** — **Medium**. **Confidence: High**.

### B051 — Select-all checks cardinality, not visible membership
- **Finder:** Hidden selections can make an entirely unselected filtered list appear fully selected.
- **Skeptic:** ACCEPT; cross-filter selection retention can be intentional, but equal sizes cannot prove membership.
- **Analysis:** `HistoryTab.tsx:48–64,293` compares selectedIds.size to filtered.length for both checked state and toggle. A disjoint equally sized set gives the wrong action. Derive checked/indeterminate/toggle behavior from visible IDs and keep any hidden-selection policy explicit.
- **VERDICT: REAL BUG** — **Low**. **Confidence: High**.

### B052 — Snippet Test Insert does not test expansion
- **Finder:** Dynamic snippet variables are pasted literally in the test path.
- **Skeptic:** ACCEPT; generic paste is legitimate but is not the promised snippet test.
- **Analysis:** `SnippetsTab.tsx:178–181` calls pasteClipboard with raw text. Voice use reads clipboard and calls expand_snippet_template first (`dictation.rs:1108–1113`). Reuse expansion for the explicit test before focus handoff/injection; read clipboard before the inserted text overwrites it.
- **VERDICT: REAL BUG** — **Medium**. **Confidence: High**.

### B053 — Mic-test bars have zero height
- **Finder:** Meter colors change on empty spans whose height is zero.
- **Skeptic:** ACCEPT; flex-1 controls width and items-end disables cross-axis stretching.
- **Analysis:** `MicTest.tsx:144–152` gives the parent height but no child height, content or stretching, and inspected CSS supplies no matching rule. Give the bars a visible height or controlled amplitude geometry. Native layout was not measured during arbitration.
- **VERDICT: REAL BUG** — **Low**. **Confidence: High**.

### B054 — Resolved event listeners leak on unmount
- **Finder:** App subscriptions only clean up if registration completes after unmount.
- **Skeptic:** ACCEPT; the late-registration race is handled, normal unmount is not.
- **Analysis:** `App.tsx:74–88` discards unlisten handles obtained while mounted. Later cleanup only flips cancelled and removes DOM listeners. Retain and release successful native registrations, while keeping existing late-resolution cleanup. Low reflects limited ordinary root remount frequency.
- **VERDICT: REAL BUG** — **Low**. **Confidence: High**.

## Native integration, packaging and test reliability

### B055 — Installed GNOME hotkey requires development files
- **Finder:** Packaged GNOME lacks the only shortcut helper and its launcher hardcodes a checkout.
- **Skeptic:** ACCEPT; successful development ancestor discovery does not make an installed package work.
- **Analysis:** GNOME skips native registration, then searches ancestors for an unbundled scripts directory and returns overall success if absent. Supplied launch script changes into a developer path and runs npm development mode. Provide an installed helper/CLI entry point with correct data/socket/executable locations and propagate registration errors; retain development support deliberately.
- **VERDICT: REAL BUG** — **Medium**. **Confidence: High**.

### B056 — Typed empty GNOME array is parsed as a path
- **Finder:** @as [] becomes a bogus custom keybinding slot.
- **Skeptic:** ACCEPT as settings-data corruption; exact settings-daemon reaction is untested.
- **Analysis:** `hotkey.rs:254–279` removes brackets only at string edges, so the GVariant type prefix survives and is normalized into a path. Parse the supported typed-array syntax and preserve actual existing slots; reject malformed output rather than write it back.
- **VERDICT: REAL BUG** — **Low**. **Confidence: High**.

### B057 — Failed shortcut replacement disarms old macOS gesture
- **Finder:** Replacing a double-tap with a failing ordinary shortcut rolls back the label but not the listener.
- **Skeptic:** ACCEPT; register can even early-return for the old stored string, leaving a naïve retry ineffective.
- **Analysis:** `hotkey.rs:36–65` disarms before fallible registration; `set_settings` restores only settings. Commit runtime replacement only after success, or explicitly restore the previous gesture/registration on failure. Static rollback defect is clear; native macOS behavior was not exercised.
- **VERDICT: REAL BUG** — **Medium**. **Confidence: High**.

### B058 — Hold-to-talk worker ordering is not event ordering
- **Finder:** A release worker can lock before the earlier press worker and discard stop.
- **Skeptic:** ACCEPT; mutual exclusion does not impose spawn order.
- **Analysis:** Each callback independently spawns a thread and acquires the mutex inside it. Release can observe inactive, return, and then press starts a session with no future release. Queue events at receipt or sequence desired state before scheduling; sharing a mutex alone does not fix this race.
- **VERDICT: REAL BUG** — **Medium**. **Confidence: High**.

### B059 — AppImage autostart persists a temporary mount path
- **Finder:** Linux current_exe is not stable across mounted AppImage launches.
- **Skeptic:** ACCEPT only for mounted AppImages; normal deb/extracted installs differ.
- **Analysis:** `autostart.rs:25–33` persists current_exe and ignores the stable AppImage launcher path. After unmount the desktop entry can point to a nonexistent executable. Resolve a validated stable APPIMAGE launcher for that packaging mode and correctly serialize the desktop Exec field; retain current_exe for normal installations.
- **VERDICT: REAL BUG** — **Medium**. **Confidence: High** for the packaging-specific defect; a bundle was not launched in this review.

### B060 — Autostart plist embeds unescaped XML text
- **Finder:** Valid installation paths containing & or < produce invalid plist XML.
- **Skeptic:** ACCEPT as a path serialization edge case.
- **Analysis:** `autostart.rs:81–95` interpolates the executable directly into a string element. Filesystem validity does not imply XML text validity. Serialize/escape XML and test round-trip values; no platform execution is needed to establish malformed XML.
- **VERDICT: REAL BUG** — **Low**. **Confidence: High**.

### B061 — Experimental GPU workflow is internally inconsistent
- **Finder:** Archive handling, feature selection, runtime libraries and artifact path block the promised build.
- **Skeptic:** ACCEPT; experimental status does not excuse deterministic dispatch failures.
- **Analysis:** Workflow saves libs.archive then tests its name for .zip, retains default static CPU features, omits the runtime library injection, and uploads from src-tauri/target despite a root workspace. The documented app-level gpu-shared feature is absent. Repair the complete feature/link/package/upload chain and validate archive formats/output paths; do not claim GPU acceleration without native runtime validation.
- **VERDICT: REAL BUG** — **Medium**. **Confidence: High**.

### B062 — Model tests share mutable global directory state
- **Finder:** Parallel fixtures race on XDG_DATA_HOME and fail to restore the original value.
- **Skeptic:** ACCEPT as test isolation, not demonstrated user-data loss.
- **Analysis:** Multiple model tests independently set/remove the process environment variable with no common guard. Default parallel execution permits one fixture to redirect another's path resolution. Inject a test directory or use one shared scope with restoration for every environment-dependent test; a single passing baseline does not remove the race.
- **VERDICT: REAL BUG** — **Low**. **Confidence: High**.

### B063 — FastConformer model-type hint allegedly misroutes native setup
- **Finder:** A transducer hint accompanies the CTC configuration on cold start and might cause native rejection/misdispatch.
- **Skeptic:** DISPROVE; exact pinned native factories select from populated model fields.
- **Analysis:** The hint inconsistency exists, but the reported functional consequence does not. Sherpa 1.13.4's [recognizer factory](https://github.com/k2-fsa/sherpa-onnx/blob/v1.13.4/sherpa-onnx/csrc/online-recognizer-impl.cc#L108) selects CTC from nemo_ctc.model, its [CTC model factory](https://github.com/k2-fsa/sherpa-onnx/blob/v1.13.4/sherpa-onnx/csrc/online-ctc-model.cc#L30) selects NeMo from the same field, and [validation](https://github.com/k2-fsa/sherpa-onnx/blob/v1.13.4/sherpa-onnx/csrc/online-model-config.cc#L187) follows that field. The app's supported provider configuration does not introduce the QNN-specific hint branch. Removing an irrelevant hint is optional cleanup, not a confirmed bug fix.
- **VERDICT: NOT A BUG** — dismissed. **Confidence: High**.

### B064 — Handsfree onset confirmation drops consumed audio
- **Finder:** Debounce/VAD latency excludes initial speech samples from recognition.
- **Skeptic:** ACCEPT; noise rejection is intentional, but dropping a confirmed utterance's onset is unnecessary.
- **Analysis:** `dictation.rs:1516–1561` consumes each batch and appends only once two positive observations activate the utterance. No pre-roll recovers the first positive batch or detector latency. Preserve a bounded pre-roll and prepend it once when onset is confirmed, without including stale prior-session/wake audio.
- **VERDICT: REAL BUG** — **Medium**. **Confidence: High** for sample exclusion; audible loss varies with timing.

## Final summary

- **Candidates adjudicated:** 64.
- **Confirmed real bugs:** 63 — B001–B062 and B064.
- **Dismissed:** 1 — B063.
- **Confirmed severity totals:** 6 Critical, 47 Medium, 10 Low. B019 is upgraded from Medium because the supported dictionary path can terminate the process.
- **Confirmed severity list:**
  - **Critical:** B001, B017, B019, B026, B027, B032.
  - **Medium:** B002–B012, B014–B016, B018, B020–B025, B028–B031, B033–B043, B046–B048, B050, B052, B055, B057–B059, B061, B064.
  - **Low:** B013, B044, B045, B049, B051, B053, B054, B056, B060, B062.
- **Score accounting:** Original accepted Finder points total 300; severity-adjusted confirmed points total 305 after B019's upgrade. Skeptic's single dismissal is supported by this arbitration (+5 under that role's scoring if judged correct). The Arbiter's requested +1/−1 score cannot be calculated without the user's hidden ground truth; no external correctness score is claimed.

Partial allegations rejected without dismissing their surviving root defect: B013's inevitability; B016's implication that every integer-default microphone fails; B026's unproven remote/speech exploit chain; B029's universal target claim; B038's Cancel All omission; B039's silent startup-caption event example; B042's text-export reference; B044's normal snippet-history example; B051's assumption that retaining hidden selections is itself wrong.

The supplemental KWS inverse lock order remains excluded: the preceding review did not establish concurrent production reachability through current callers. Avoid that order when implementing settings/service lifecycle changes, without counting it as a proven additional bug. This audit establishes the listed source defects; it does not certify that no other bugs exist or that Windows/macOS/GPU/native focus/audio-server failure cases were executed. Subsequent fixes need grouped regression tests, targeted native validation where available, and explicit reporting of unavailable platform coverage.
