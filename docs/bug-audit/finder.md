# Bug Finder report

Audit date: 2026-10-19. Role 1 only: candidate discovery, before Skeptic/Arbiter review and before production fixes. Scope is the entire OpenDictate repository, with no special interviewer/client emphasis. Locations refer to the source as inspected at audit start. These are **candidate bugs**, not claims of externally verified ground truth or exhaustive absence of other defects.

Scoring: Low = 1, Medium = 5, Critical = 10. Related symptoms sharing a root cause are grouped. “Static” means the concrete code path was traced; it does not mean that native hardware behavior was exercised. Platform-dependent and timing-dependent claims are marked for the next reviewer.

## Recording, recognition and session ownership

### B001 — Microphone test can execute destructive voice actions
- **Location:** `src-tauri/src/dictation.rs:769-801`; `src/components/MicTest.tsx` start/stop handlers.
- **Issue/evidence:** `process_utterance` calls `handle_voice_action_or_snippet` at line 783 before its `if test` check at 792. A test recording recognized as “clear all”, “scratch that”, a snippet command, or an auto-submit command executes input/history side effects. It also invokes configured cloud polish before the test guard. The ordinary mic test is offered before model setup, but its stop path still requires `load_engine`, so an otherwise working mic can end in a missing-model error.
- **Trigger:** With voice actions enabled, speak a command during a mic test, then stop. The action path runs even though this session is marked test.
- **Impact:** Critical. **Points awarded:** 10. **Confidence:** High, static call order; no destructive command executed during audit.

### B002 — Manual streaming stop does not flush the final audio correctly
- **Location:** `src-tauri/src/dictation.rs:476-502,645-678`; `crates/opendictate-core/src/stt/streaming.rs:239-254`.
- **Issue/evidence:** Stop drains only currently ready frames; it never calls the stream's `input_finished` or supplies tail padding. The final incomplete recognition chunk can therefore be omitted. It also snapshots the capture tail before stopping the microphone. A second race exists when the loop advances `watermark`, drops the pipe mutex, and stop takes the mutex before that batch is accepted: stop skips that batch and then removes the pipe.
- **Trigger:** Release the hotkey immediately after speech, especially short utterances or a stop between the loop's two pipe locks. Sherpa's documented finalization example adds silence, calls `input_finished`, then drains (source below).
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** High for missing finalization; Medium for race timing.

### B003 — Streaming transcripts take inconsistent processing paths
- **Location:** `src-tauri/src/dictation.rs:522-545,678-703,775-801`.
- **Issue/evidence:** Manual streaming stop only maps punctuation and handles snippets, then commits. It skips general voice actions and AI polish, which the streaming endpoint path invokes. Both streaming commit paths omit the dictionary-casing/clean-text sequence used offline. Thus “scratch that” can be typed as literal text when stopped manually but execute Undo when an endpoint fires; enabled polish/dictionary behavior depends on stop timing/model.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** High, static branch comparison.

### B004 — Streaming caption fallback is unreachable after draining
- **Location:** `src-tauri/src/dictation.rs:498-520`; `crates/opendictate-core/src/stt/streaming.rs:248-254`.
- **Issue/evidence:** `drain` decodes while `is_ready`; immediately afterward the only partial-emission block requires `is_ready` again. No additional audio is accepted between these checks. If the auxiliary caption model is missing or failed, the selected streaming recognizer produces no live partial events despite generating text.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** High, static invariant.

### B005 — Continuous caption worker survives stop and shares the next session
- **Location:** `src-tauri/src/dictation.rs:127-143,565-640`.
- **Issue/evidence:** Caption worker captures `continuous` once at line 606; its loop condition remains true when that captured value is true even after recording stops. Ordinary `stop` clears the continuous flag but never clears `caption_active`. Starting again replaces the global pipe and creates another worker; old workers can consume the new pipe, and worker cleanup unconditionally clears global state without a session identity. Quick stop/start has the same stale-worker risk even without continuous mode.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** High for continuous-worker leak; Medium for timing consequences.

### B006 — UI commands and hotkeys do not share a session-operation boundary
- **Location:** `src-tauri/src/commands.rs:240-267`; `src-tauri/src/hotkey.rs:104-218`; `src-tauri/src/dictation.rs:35-55,127-190`; `src/lib/useRecording.ts:5-40`.
- **Issue/evidence:** The serializer protects hotkey workers only. UI start/stop/cancel use independent blocking tasks. `start` cancels an existing recorder, and `stop` announces recording=false before inference finishes. A UI start concurrent with hotkey start/stop can cancel or replace another operation's recording. A cancel during inference cannot invalidate that inference's later history write/paste. Each `useRecording` instance also has its own busy flag, so Header/Home do not serialize each other.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** Medium; explicit interleavings identified, full native race not exercised.

### B007 — Streaming warmup creates a real recording pipeline
- **Location:** `src-tauri/src/commands.rs:217-228`; `src-tauri/src/dictation.rs:381-454`; `src/components/ModelCard.tsx` model-selection handler.
- **Issue/evidence:** Selecting a streaming model calls `warmup_model`, which invokes `spawn_streaming` rather than only loading/caching the recognizer. That function replaces the shared stream/session, sets streaming active and starts its loop. If selection happens during recording, warmup can consume/commit existing audio under the new model; if recording starts while the idle warmup loop is exiting, stale cleanup can clear the new session's active flag.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** High for side effects; Medium for interleavings.

### B008 — Handsfree and explicit recording both process the same microphone
- **Location:** `src-tauri/src/dictation.rs:35-48,119,1458-1617`.
- **Issue/evidence:** Explicit start keeps the handsfree recorder open and clears its buffer, but the handsfree loop never checks `user_dictation_active` or test mode. Awake handsfree continues recognizing/committing the same speech while explicit dictation or a mic test is running. Sleeping handsfree can also wake during that session. This can produce duplicate text, duplicate commands, or actions during a test.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** High, static ownership checks absent.

### B009 — Handsfree enable failures leave false success and a wedged active flag
- **Location:** `src-tauri/src/commands.rs:466-470,502-519`; `src-tauri/src/dictation.rs:1402-1430`; `src-tauri/src/lib.rs:150-152`.
- **Issue/evidence:** Settings ignores `start_handsfree` errors and persists handsfree=true even if KWS is missing. The dedicated toggle also persists before start succeeds. Start sets `handsfree_active=true` before microphone startup, so a device-open failure leaves no worker but causes subsequent enable attempts to return early as successful. Downloading KWS later does not retry the originally failed service start.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** High, static failure paths.

### B010 — Changing handsfree wake words does not update the running listener
- **Location:** `src-tauri/src/commands.rs:391-398`; `src-tauri/src/dictation.rs:1407,1429-1433,1489,1497`.
- **Issue/evidence:** Wake-word changes clear the cached KWS engine, but the running loop owns its old `Arc<Spotter>` and creates all subsequent sessions from it. Old phrases continue working and newly saved phrases do not work until service restart. Similar stale microphone selection arises because changing the mic setting does not restart the always-open handsfree recorder.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** High for wake words, static retained ownership.

### B011 — Handsfree utterance duration cap only runs during silence
- **Location:** `src-tauri/src/dictation.rs:1547-1576`.
- **Issue/evidence:** `MAX_UTTERANCE_DURATION` is tested only in the `else` branch of `is_speech`. Continuous speech keeps extending `utterance_samples` indefinitely without hitting the advertised duration bound or committing a result. Unlike the recorder's rolling buffer, this vector has no separate cap.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** High, static branch condition.

### B012 — Offline continuous mode drops speech while processing the previous utterance
- **Location:** `src-tauri/src/dictation.rs:263-282` (and analogous maximum-duration branch).
- **Issue/evidence:** The continuous loop closes the microphone, synchronously waits for inference and optional network polish, then restarts it. Speech after the pause that triggered transcription is never captured for the next utterance. The gap can last seconds or the full polish timeout while continuous listening remains enabled.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** High, static capture lifecycle; reviewer should compare intended continuous UX.

### B013 — Continuous microphone meter stops after the first utterance
- **Location:** `src-tauri/src/dictation.rs:98,267-282,1224-1234`.
- **Issue/evidence:** The sole audio-level emitter exits as soon as the recorder is stopped for an intermediate continuous utterance. `restart_recorder` does not restart the emitter. Subsequent audio can be recorded but the displayed level remains zero until a new user session.
- **Impact:** Low. **Points awarded:** 1. **Confidence:** High, static lifecycle.

### B014 — Buffer clear breaks absolute consumer watermarks
- **Location:** `crates/opendictate-core/src/audio/capture.rs:63-69,289-295`; existing test at `524-541`.
- **Issue/evidence:** `SharedBuffer::clear` advances `base` and then resets it to zero. If new audio arrives before the next consumer poll, the old absolute watermark skips that audio. Existing test polls once while the buffer is empty, masking this interleaving.
- **Safe reproduction:** Compiled the actual `SharedBuffer` code plus the exact `take_since` index calculation in a temporary Rust program: push four samples, watermark=4, clear, push `[7,8]`, read => `[]` instead of `[7,8]`.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** High, reproduced.

### B015 — Long manual recordings silently lose their beginning
- **Location:** `crates/opendictate-core/src/audio/capture.rs:11,48-59`; `src-tauri/src/dictation.rs:127-143`.
- **Issue/evidence:** The common recorder buffer drops old audio after `MAX_RECORDING_SAMPLES` (120 seconds). Manual offline recording has no duration stop, flush or warning, and transcribes only the retained buffer at stop. A recording longer than two minutes therefore loses initial dictated content.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** High, static bounded-buffer behavior.

### B016 — CPAL capture assumes every default device format is f32
- **Location:** `crates/opendictate-core/src/audio/capture.rs:298-383`.
- **Issue/evidence:** Code takes the device's default input configuration but always builds a stream with an `&[f32]` callback; it never branches on that configuration's sample format. Devices exposing integer PCM defaults can reject the stream rather than recording. Relevant to Windows/macOS and Linux CPAL fallback.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** Medium; platform-dependent, no such device available during audit.

### B017 — PulseAudio blocking waits defeat stop flags and setup deadlines
- **Location:** `crates/opendictate-core/src/audio/pulse.rs:126-151,257,341,390-400,465-479`; `crates/opendictate-core/src/audio/capture.rs:394-405`.
- **Issue/evidence:** Capture waits in `pa_threaded_mainloop_wait` when no data is available. External stop only sets an atomic flag; it does not signal that mainloop, and recorder shutdown joins the thread. A suspended/stalled source can therefore hang stop. The setup timeout also sets the flag then joins a thread which may still be in an unbounded setup wait. Source enumeration uses blocking iteration inside nominal deadline loops, so its synchronous command can freeze the UI on an unresponsive server.
- **Impact:** Critical. **Points awarded:** 10. **Confidence:** High for uncancellable waits, Medium for exact native trigger; no user audio server was disrupted.

### B018 — Capture failure leaves the logical recorder running
- **Location:** `crates/opendictate-core/src/audio/capture.rs:377-380`; `crates/opendictate-core/src/audio/pulse.rs:423-428`.
- **Issue/evidence:** CPAL error callback sets only `stop_signal`; Pulse stream failure exits its thread. Neither transitions recorder state to idle nor delivers a recorder-failed event. The UI and handsfree loop see `is_recording=true`, so a disconnected device can leave an apparently active but dead session with no automatic restart.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** High for state mismatch, platform-dependent trigger.

### B019 — Dictionary hotword boosting is configured with an unsupported decoder
- **Location:** `crates/opendictate-core/src/stt/engine.rs:175,283-292`; `crates/opendictate-core/src/stt/streaming.rs:156-164,206-210`; `src-tauri/src/dictation.rs` dictionary-hotword construction.
- **Issue/evidence:** Offline transducer and streaming recognizers use `greedy_search` but create streams with dictionary hotwords. Sherpa documents hotwords as requiring transducer `modified_beam_search`; the default greedy decoder does not support them. The raw-word hotword formatting/tokenizer configuration also needs checking before simply changing decoder mode. Post-transcription casing only repairs already-recognized words, not recognition bias.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** High, source plus current upstream documentation.

## Text processing, actions and injection

### B020 — Dictionary correction duplicates punctuation and can lose canonical casing
- **Location:** `crates/opendictate-core/src/text.rs:12-35,43-106`; `src-tauri/src/dictation.rs:780-781`; `src-tauri/src/inject.rs` `clean_text`.
- **Issue/evidence:** Matching spans exclude punctuation but replacement inserts the entire canonical dictionary term. Existing suffix punctuation remains, so dictionary `C++` applied to text `C++` produces `C++++`; `C#` similarly duplicates `#`. Afterwards generic sentence capitalization can change a first-word dictionary spelling such as `iPhone` to `IPhone`.
- **Safe reproduction:** Compiled actual function in a temporary Rust program: `correct_dictionary_terms("C++", &["C++"])` => `"C++++"`.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** High, reproduced core corruption.

### B021 — Sound-effect filtering deletes ordinary dictated content
- **Location:** `crates/opendictate-core/src/text.rs:177-292`; `src-tauri/src/inject.rs` `clean_text`.
- **Issue/evidence:** Every square-bracketed span is removed, regardless of contents. Parenthetical spans are classified using substrings (`ding`, `wind`, etc.), so ordinary words are removed too. This cleanup runs on normal offline transcripts, not just a validated Whisper annotation grammar.
- **Safe reproduction:** Actual function returned `"array  "` for `"array[0] (coding) (Windows)"`.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** High, reproduced.

### B022 — Fuzzy snippet scoring can exceed 1 and outrank an exact match
- **Location:** `crates/opendictate-core/src/text.rs:365-385,424-443`.
- **Issue/evidence:** Bigram overlap counts every occurrence from `a` if that bigram occurs anywhere in `b`, without limiting multiplicity. This can yield Dice scores greater than 1; the DP then has negative substitution costs and selects a wrong trigger over an exact trigger.
- **Safe reproduction:** Actual `fuzzy_match_trigger("aaaaa", ["aa", "aaaaa"], 0.6)` => `Some(("aa", 1.6))`.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** High, reproduced.

### B023 — Voice-command payloads are lowercased and punctuation-stripped
- **Location:** `crates/opendictate-core/src/text/actions.rs:125-130` and payload-producing branches; `src-tauri/src/dictation.rs:1063-1074`.
- **Issue/evidence:** Parser normalizes the entire transcript before extracting command arguments. A case-sensitive URL path/token is changed by “open …”; prompt-and-send text and dictated snippet contents lose casing and trailing meaningful punctuation. Normalizing the command keyword should not rewrite arbitrary payload data.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** High, static dataflow.

### B024 — AI polish accepts truncated output as a complete transcript
- **Location:** `crates/opendictate-core/src/text/polish.rs:81-95,120-139`.
- **Issue/evidence:** Requests cap output at 1024 tokens, while the response type omits `finish_reason`. A nonempty response cut off for length is treated as success and replaces the original transcript, so longer/high-token-count dictation can be silently truncated rather than retaining the original text.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** Medium; requires a length-limited provider response, not exercised against paid service.

### B025 — Selectable Local SLM polish silently does nothing
- **Location:** `crates/opendictate-core/src/text/polish.rs:50-57`; `src/components/tabs/SettingsTab.tsx` Local SLM provider option and polish modes.
- **Issue/evidence:** The UI exposes Local SLM as a working offline provider with cleanup/bullet modes, but its implementation always returns the input unchanged and logs a placeholder message. Users can enable a feature which cannot perform its stated behavior.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** High, explicit placeholder.

### B026 — App-switch and Windows URL actions interpolate recognized text into scripts
- **Location:** `src-tauri/src/inject.rs:856-875,922-927`; `src-tauri/src/dictation.rs:923-929,968-972`.
- **Issue/evidence:** macOS inserts `app_name` inside an AppleScript quoted string; Windows inserts it inside an expandable PowerShell string. No escaping/parameter separation is performed. Quotes or PowerShell substitution syntax in a recognized app name can become executable script. Windows also passes a free-form URL to `cmd /C start`, exposing shell metacharacters. Recognition text is data, not trusted script source.
- **Impact:** Critical. **Points awarded:** 10. **Confidence:** Medium; static injection sinks verified, platform execution and end-to-end recognition payload not exercised. Reviewer should distinguish this from intentionally supported fixed terminal commands.

### B027 — Failed insertion still triggers submit and reports success
- **Location:** `src-tauri/src/dictation.rs:848-980,1158-1165,1194-1198`.
- **Issue/evidence:** Action branches discard injection/commit errors. `PromptAndSend` and `TerminalCommand` press Enter even if the intended text failed to paste, potentially sending or executing pre-existing input. Other actions report success after failed key injection. `commit_text` emits `injected:true`/inserted before attempting the paste, so main-window success state can disagree with the actual failure.
- **Impact:** Critical. **Points awarded:** 10. **Confidence:** High for unchecked result and Enter sequence; no real terminal command executed.

### B028 — All clipboard backends can fail but paste still runs
- **Location:** `src-tauri/src/inject.rs:117-122` and `inject_text`.
- **Issue/evidence:** Linux clipboard helper explicitly returns `Ok(())` when both native clipboard and every external tool fail. The caller then pastes whatever was previously on the clipboard and records the new transcript as inserted. This is wrong-content insertion, not a harmless fallback.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** High, static error handling.

### B029 — Generic key bindings do not implement advertised actions in common targets
- **Location:** `src-tauri/src/inject.rs:360-377,419-423,664-674`.
- **Issue/evidence:** Linux always pastes using Ctrl+V, which common terminal targets interpret as quoted input rather than paste (their paste command is Ctrl+Shift+V). Delete-line uses Ctrl+U on Linux, which is a browser command rather than line deletion in ordinary web inputs; Windows delete-line sends Ctrl+Backspace, deleting a word. These fixed bindings are not target-aware despite actions being exposed across applications.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** Medium; target/platform-dependent, assess expected supported targets.

### B030 — Several wtype fallbacks release a nonexistent modifier instead of typing a key
- **Location:** `src-tauri/src/inject.rs:373,384,422,429,445` and related tab actions.
- **Issue/evidence:** Calls such as `wtype -M ctrl -m z` use `-m z` as if it presses Z. Installed wtype documentation defines `-m` as release-modifier (valid values include ctrl/shift, not z/u/a/c) and `-k` as type-key. On Wayland systems where uinput/ydotool are unavailable these action fallbacks fail, with xdotool often unable to control the native target either. The first paste wtype call is correct; the reported bug concerns the malformed variants, not every wtype path.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** High, checked installed man page.

### B031 — Main-window Insert/Undo actions operate on OpenDictate's own focus
- **Location:** `src/components/tabs/SnippetsTab.tsx:178-181`; `src/components/tabs/HomeTab.tsx:337-340`; `src-tauri/src/commands.rs` `paste_clipboard` / `undo_last_insert`; `src-tauri/src/inject.rs` injection helpers.
- **Issue/evidence:** Clicking these main-window controls focuses OpenDictate, then the backend immediately sends paste/undo to the currently focused application. There is no remembered target window or restoration step. Snippets promises insertion into the focused app; Home Undo can undo an edit in OpenDictate rather than the external dictation insertion. Dock is nonfocusable, but these controls are not in the dock.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** Medium; native focus behavior needs platform review.

## Model storage and download lifecycle

### B032 — Model removal accepts paths outside the model directory
- **Location:** `crates/opendictate-core/src/stt/models.rs:292-297,1080-1093`; `src-tauri/src/commands.rs:159-167,211-214`.
- **Issue/evidence:** `remove_model` accepts arbitrary `id`, joins it to the model directory, then recursively removes the result without catalog validation or containment checks. Absolute IDs replace the root; `../` IDs escape it. Both renderer-accessible remove and cancel commands reach this function, so a malformed command can delete unrelated user files with app permissions.
- **Impact:** Critical. **Points awarded:** 10. **Confidence:** High for path traversal; no unrelated files deleted. No claim of an independently demonstrated remote renderer compromise.

### B033 — Cancel/retry lets an old downloader delete a replacement download
- **Location:** `src-tauri/src/commands.rs:76-155,159-167`; `crates/opendictate-core/src/stt/models.rs` cancellation cleanup and shared temp paths.
- **Issue/evidence:** Cancel removes the active map entry and deletes model files immediately, before the worker acknowledges its flag. A new download is then admitted for the same ID and paths. When the old worker exits, its unconditional map removal/cleanup can remove the new worker's entry or files. Extraction/install also shares destinations with the retry.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** High for ownership race; timing-dependent, network reproduction not run.

### B034 — Background caption/VAD installs bypass download deduplication
- **Location:** `src-tauri/src/lib.rs:154-165`; `src-tauri/src/dictation.rs:99-113`; `src-tauri/src/commands.rs:112-122`; `crates/opendictate-core/src/stt/models.rs:867-887`.
- **Issue/evidence:** Startup and each dictation without captions call core `ensure_model` directly; manual installs have a separate command-level active map. Concurrent downloads/extractions use the same archive, `.part`, extraction and model paths. Multiple STT installs similarly race their automatic VAD install. A slow first install plus recording/manual download can cause truncation, rename/extraction failures or deleting the other worker's files.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** High, static concurrent entry points and identical paths.

### B035 — A stalled download body cannot observe cancellation
- **Location:** `crates/opendictate-core/src/stt/models.rs:524-534,560-584`.
- **Issue/evidence:** Only connect timeout is configured. The blocking response/header/body operations have no deadline, and cancellation is checked outside the blocking read. A server that connects then stops sending can keep the worker blocked indefinitely after the UI says cancelled. Installed ureq source has `recv_response`, `recv_body` and global timeouts defaulting to None.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** High, code plus installed dependency defaults; stalled endpoint not exercised.

### B036 — KWS readiness accepts empty model files
- **Location:** `crates/opendictate-core/src/stt/models.rs:401-422`.
- **Issue/evidence:** Each size-validation check falls back to a filename-only search. Even canonical zero-byte `encoder.onnx`, `decoder.onnx`, `joiner.onnx` satisfy that fallback as long as tokens is present. Interrupted/failed installs can be reported installed and skipped by ensure, then fail at recognizer construction.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** High, static OR-condition bypass.

### B037 — Download completion does not refresh catalog or preserve installation phase
- **Location:** `src/lib/store.ts:149-151`; `src/App.tsx:48-58`; `src/components/ModelCard.tsx:82-137`; `src/components/tabs/SettingsTab.tsx:255-266`.
- **Issue/evidence:** Progress is removed when network bytes reach total, although extraction/VAD install may still be running. `models-ready` refreshes status but not the catalog outside onboarding, so a completed model can keep showing Download instead of Use. A cached/fast install without an intermediate progress row can also leave `startingDownloads` stuck. Settings awaits a command that only starts a background worker, then immediately toasts that the model is ready and refreshes too early.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** High, event/state trace.

### B038 — Download All's wait loop reads a stale starting set
- **Location:** `src/components/ModelCard.tsx:165-204,206-218`.
- **Issue/evidence:** The async handler's interval closes over the render's `startingDownloads`, usually empty before the first `setStartingDownloads`. Before a response/progress event arrives, `!inProgress && !startingDownloads.has(id)` is already true, so the next model starts after 500ms rather than waiting. End-of-bytes progress removal causes the same advance during extraction. Errors swallowed by `handleDownload` are treated as completed steps; Cancel All only sees active progress IDs, missing connecting downloads.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** High, closure/state flow.

### B039 — Onboarding advances on unrelated model completion without selecting the downloaded STT model
- **Location:** `src/components/Onboarding.tsx:19,27-31`; `src-tauri/src/commands.rs` `models_status` / `ensure_model`; `src/components/ModelCard.tsx` download versus Use handlers.
- **Issue/evidence:** Any `models-ready` event advances the speech-model step, including VAD/KWS/caption completion. Readiness is “some STT installed”, not “selected STT installed”. Downloading a nondefault model does not select it, but its completion moves to the next step before the user presses Use. First dictation can then fail on the still-selected, missing default model.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** High, static onboarding transition.

### B040 — One model's CPU benchmark disables different streaming models
- **Location:** `src-tauri/src/lib.rs:170-178`; `src/components/ModelCard.tsx:50-51,438-451`.
- **Issue/evidence:** Backend benchmarks only the Parakeet streaming model into a single global number. UI applies its “too slow” result to every streaming card and suppresses Use. A slow Parakeet CPU result can prevent selecting the different FastConformer streaming model or a model to run with GPU acceleration.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** High, benchmark ID versus global UI predicate.

## Database, settings and frontend state

### B041 — CSV export swaps transcript and timestamp columns
- **Location:** `src-tauri/src/commands.rs:829-842`.
- **Issue/evidence:** Header is `id,text,created_at,duration_ms,source`, but each row writes `id,created_at,text,duration_ms,source`. Any consumer reading named columns receives timestamps as text and transcript strings as dates. The existing test only checks substrings and misses column mapping.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** High, direct serialization mismatch.

### B042 — History export silently stops at 500 rows
- **Location:** `src-tauri/src/commands.rs:796-799`; `src-tauri/src/db.rs:111-114`.
- **Issue/evidence:** Export reuses the display query with `LIMIT 500`, although the database retains older records. CSV/JSON/text exports therefore omit older history without warning. Using export as backup before Clear All loses those records permanently.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** High, query limit.

### B043 — Editing/deleting pre-reset history corrupts post-reset statistics
- **Location:** `src-tauri/src/db.rs:138-183,288-299`.
- **Issue/evidence:** Reset deletes daily aggregates but retains history and only records a boolean backfill marker. Later delete/edit adjusts statistics by the old row's full word count/delta even if that row was excluded by the reset. Example: old 100-word entry, reset, new 3-word entry the same day, delete old entry => new day's words/sessions become zero rather than 3/1.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** High, exact SQL sequence.

### B044 — Word statistics count literal spaces rather than words
- **Location:** `src-tauri/src/db.rs:53-60,97-106,127-135`.
- **Issue/evidence:** SQL and Rust count spaces + 1. `one\ntwo` is counted as one word, while repeated spaces/whitespace-only text are overcounted. Multiline snippets and AI bullet formatting make this reachable; frontend whitespace-based counts then disagree with the aggregate dashboard.
- **Impact:** Low. **Points awarded:** 1. **Confidence:** High, deterministic counting.

### B045 — Deleting today's last entry incorrectly resets an existing streak
- **Location:** `src-tauri/src/commands.rs:749-766`; `src-tauri/src/db.rs:150-154`.
- **Issue/evidence:** A missing today row permits a streak ending yesterday; an existing today row with zero words does not. Delete leaves that zero row, so deleting today's only entry breaks yesterday's active streak even though the equivalent never-recorded-today case preserves it.
- **Impact:** Low. **Points awarded:** 1. **Confidence:** High, branch difference.

### B046 — Reset Settings does not apply the runtime/OS reset
- **Location:** `src-tauri/src/commands.rs:193-208` versus `466-497`.
- **Issue/evidence:** Reset replaces/persists defaults and re-registers hotkey/repositions dock, but does not stop handsfree, disable OS autostart, or invalidate provider/KWS caches. Settings can display handsfree/autostart off while microphone listening and startup registration remain active. Hotkey reset failure is also discarded, leaving a displayed default which may not work.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** High, missing side effects relative to normal save.

### B047 — UI hydration treats handsfree microphone ownership as an explicit recording
- **Location:** `src-tauri/src/commands.rs:270-272`; `src/lib/store.ts:184-204`; `src-tauri/src/dictation.rs:116-123`.
- **Issue/evidence:** Events use `user_dictation_active`, but `is_recording` returns raw recorder hardware state. Sleeping handsfree holds the mic open, so refresh/reload sets the UI recording=true even though no explicit session exists. Record becomes Stop and can transcribe/cancel the background capture rather than starting the intended session.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** High, inconsistent state definitions.

### B048 — Dock label is absent from Tauri capabilities
- **Location:** `src-tauri/capabilities/default.json:5`; `src-tauri/tauri.conf.json` dock window; `src/App.tsx:45-73`; `src/components/DockButton.tsx:40-77`.
- **Issue/evidence:** Capability applies to `main` and `overlay`, but the real second window is `dock`. Core event-listen calls therefore lack that window's capability grant. Custom DOM fallback handles overlay/partial only, not recording-changed/audio-level. With `recording=false` but state=`listening`, Dock considers itself active yet `canStop=false`, so clicking does nothing. Verify behavior in native Tauri because capability details are framework-sensitive.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** Medium; label mismatch certain, native ACL symptom requires review.

### B049 — Dock success/error flash can remain indefinitely
- **Location:** `src/components/DockButton.tsx:26-38,73-84`.
- **Issue/evidence:** Effect cleanup cancels the flash-clear timer whenever overlay state changes, but non-inserted/non-error states do not clear the flash. If a new recording begins before the 1400ms success timer, success styling persists and takes priority over active recording or later status until another timer clears it.
- **Impact:** Low. **Points awarded:** 1. **Confidence:** High, effect cleanup sequence.

### B050 — Last-result Undo stays disabled for later dictations
- **Location:** `src/components/tabs/HomeTab.tsx:308-346`.
- **Issue/evidence:** Local `undone` is set true after Undo and never reset when `lastResult` changes. All subsequent results show “Undone” and disabled Undo until the component is remounted.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** High, state lifecycle.

### B051 — History select-all uses selection count instead of membership
- **Location:** `src/components/tabs/HistoryTab.tsx:48-64,293`.
- **Issue/evidence:** Selection survives search changes. If hidden selected IDs happen to equal the new filtered list's length, Select All appears checked even when no visible row is selected; clicking clears old selections instead of selecting current results. Bulk deletion can still target hidden selections.
- **Impact:** Low. **Points awarded:** 1. **Confidence:** High, deterministic UI state.

### B052 — Snippet Test Insert skips dynamic template expansion
- **Location:** `src/components/tabs/SnippetsTab.tsx:178-181`; `src-tauri/src/commands.rs` `paste_clipboard`; `src-tauri/src/dictation.rs` snippet expansion path.
- **Issue/evidence:** Test Insert passes raw snippet text to generic paste instead of the snippet expansion pipeline. `{date}`, `{time}` and `{clipboard}` are inserted literally, so the offered test does not test what voice insertion will produce.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** High, distinct backend paths.

### B053 — Microphone test meter bars have zero height
- **Location:** `src/components/MicTest.tsx:144-152`.
- **Issue/evidence:** Empty spans inside an `items-end` flex container receive width but no height/min-height or vertical stretch. Background color changes are invisible because the bars have zero intrinsic height. This hides the visual feedback in the onboarding mic test.
- **Impact:** Low. **Points awarded:** 1. **Confidence:** High from layout constraints; browser geometry not measured in this role.

### B054 — Application event subscriptions leak after ordinary unmount
- **Location:** `src/App.tsx:74-88`.
- **Issue/evidence:** Resolved `unlisten` functions are invoked only if registration finishes after cleanup. When registration succeeds while mounted, the handles are discarded; later cleanup cannot unregister them. Remount/HMR leaves callbacks alive and duplicates updates, even though the late-registration race is handled.
- **Impact:** Low. **Points awarded:** 1. **Confidence:** High, resource lifecycle; ordinary production root usually mounts once.

## Native integration, packaging and test reliability

### B055 — Packaged GNOME hotkeys depend on development-only scripts
- **Location:** `src-tauri/src/hotkey.rs:39-48,239-245,290-301`; `src-tauri/tauri.conf.json` bundle; `scripts/opendictate-toggle.sh`; `scripts/opendictate-launch.sh`.
- **Issue/evidence:** GNOME disables native shortcut registration and relies entirely on a script found by walking parent directories for `scripts/`. The scripts are not bundled resources. Installed deb/AppImage layouts therefore lack the required helper. The supplied launcher also hardcodes the developer checkout and `npm run tauri dev`, and toggle hardcodes `~/.local/share` rather than the actual configured data directory. Registration still reports success when the helper is absent.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** High, packaging/static discovery path.

### B056 — GNOME empty keybinding array is parsed as a path
- **Location:** `src-tauri/src/hotkey.rs:254-279,341-342`.
- **Issue/evidence:** The parser expects bare `[]`, but GVariant's empty typed array representation can be `@as []`. Stripping only leading `[`/trailing `]` then splitting yields a bogus `@as [/` slot, which is written alongside the real binding path. First-time GNOME setup can retain an invalid path or fail to apply the expected configuration. Review severity against actual settings-daemon validation.
- **Impact:** Low. **Points awarded:** 1. **Confidence:** Medium; typed-array parsing certain, resulting GNOME behavior not executed.

### B057 — Failed macOS shortcut replacement leaves the old double-tap disarmed
- **Location:** `src-tauri/src/hotkey.rs:36-37,49-65`; `src-tauri/src/commands.rs:451-462`.
- **Issue/evidence:** Replacing a double-tap hotkey with an ordinary shortcut first disarms double-tap. If ordinary shortcut registration fails (e.g. conflict), settings rolls back the string but does not re-arm the prior gesture. UI shows the old shortcut restored although it no longer works.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** High, static failure/rollback order; macOS not run.

### B058 — Hold-to-talk mutex does not preserve press/release ordering
- **Location:** `src-tauri/src/hotkey.rs:104-218`.
- **Issue/evidence:** Each press/release independently spawns a thread which later obtains the mutex. A quick release thread can acquire it before the press thread is scheduled, observe inactive and return; the press then starts recording with no remaining release. Mutual exclusion prevents overlapping operations but does not guarantee event order.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** Medium, scheduler-dependent interleaving; no stress repro in native hotkey loop.

### B059 — AppImage autostart stores a temporary mounted executable path
- **Location:** `src-tauri/src/autostart.rs:25-33`.
- **Issue/evidence:** Linux autostart uses `current_exe`, which for an AppImage launch normally points inside its temporary mount. The desktop entry persists that mount path instead of the stable AppImage launcher (`APPIMAGE`), so the next login cannot launch it after the mount disappears.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** Medium; packaging-specific, AppImage not launched during audit.

### B060 — macOS autostart plist does not XML-escape its executable path
- **Location:** `src-tauri/src/autostart.rs:81-95`.
- **Issue/evidence:** Executable path is inserted raw into a plist `<string>`. An installation parent containing `&` or `<` produces invalid XML, breaking launch-at-login even though writing the file succeeds.
- **Impact:** Low. **Points awarded:** 1. **Confidence:** High, deterministic serialization; platform not run.

### B061 — Experimental GPU workflow cannot produce the promised usable artifact
- **Location:** `.github/workflows/gpu-build.yml:44-68`; `crates/opendictate-core/Cargo.toml:13-16`; `src-tauri/Cargo.toml` features; `scripts/inject-linux-libs.py`; `docs/GPU.md` build instructions.
- **Issue/evidence:** Archive is always saved as `/tmp/libs.archive`, then extension-matched against `*.zip`, so ZIP input goes to tar. Build supplies GPU shared libraries but never disables default static CPU linking or enables core `gpu-shared`; the app crate does not expose the documented unqualified feature. Runtime shared-library bundling/injection is not invoked. Upload uses `src-tauri/target/...`, while this Cargo workspace emits under root `target/...`. These are independent blockers within the same GPU artifact workflow; one score avoids counting each line as a separate product issue.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** High for extension/feature/output-path mismatches; no network CI build run.

### B062 — Model tests race on global XDG_DATA_HOME
- **Location:** `crates/opendictate-core/src/stt/models.rs:1115-1117,1201-1211,1233-1259,1289-1301,1445-1448`.
- **Issue/evidence:** Parallel tests overwrite/remove the same process environment variable without a shared serial guard or restoring the original value. A test may resolve another test's temporary model directory or fall back to the user's normal directory mid-assertion. This makes model install/removal tests unreliable and can make model-dependent tests accidentally inspect a different fixture.
- **Impact:** Low. **Points awarded:** 1. **Confidence:** High for isolation race; test-only impact. Parent's baseline run passed, which does not disprove the interleaving.

### B063 — Cold FastConformer setup receives a transducer model-type hint
- **Location:** `src-tauri/src/dictation.rs:395-414`; `crates/opendictate-core/src/stt/streaming.rs:126-151`.
- **Issue/evidence:** Empty-cache construction always supplies `Some("nemo_transducer")`, including selected NeMo CTC FastConformer. Changing from a cached different model instead supplies None. The CTC config forwards that hint alongside `nemo_ctc`, creating inconsistent native config for the same model depending on startup/cache state. It may cause rejection or incorrect model dispatch if the native factory honors the hint.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** Low; deliberately retained for Skeptic to inspect native dispatch semantics. No assertion that CTC actually fails without checking the library factory.

### B064 — Handsfree onset confirmation discards the beginning of speech
- **Location:** `src-tauri/src/dictation.rs:1516-1524,1547-1561`.
- **Issue/evidence:** Samples are appended only after consecutive speech frames confirm an utterance. No pre-roll stores the first positive frame or audio while neural VAD is deciding. Those consumed samples cannot enter the final utterance, clipping leading phonemes/short first words. This is distinct from the duration-cap issue B011.
- **Impact:** Medium. **Points awarded:** 5. **Confidence:** Medium; data exclusion is static, audible impact depends on VAD delay and utterance.

## Evidence sources and safe checks

- Temporary Rust program compiled the actual repository text/tokenization functions and `SharedBuffer` implementation without modifying runtime source or user data. Results: `C++` → `C++++`; `array[0] (coding) (Windows)` → `array  `; exact snippet trigger lost to a score of 1.6; clear+append before polling lost `[7,8]`.
- Current Sherpa documentation fetched via Context7: [online recognition finalization example](https://github.com/k2-fsa/sherpa-onnx/blob/master/python/sherpa_onnx/online_recognizer.py) and [hotwords/decoder requirements](https://k2-fsa.github.io/sherpa/onnx/hotwords/index.html). Finalization requires end-of-input handling; transducer hotwords require modified beam search. Native CTC model dispatch remains an explicit unresolved candidate.
- Read installed ureq 3 source to confirm default response/body timeouts are None, and installed wtype man page to confirm modifier/key option semantics. No native keystrokes, destructive action, microphone disruption, paid provider call or large model download was run.
- Baseline validation is parent-owned in `validation.md`: frontend build passed; Rust workspace library tests passed (98 passed, 3 ignored); non-strict Clippy passed with existing warnings. Passing current tests does not cover most reported runtime interleavings/UI paths.

## Coverage map and limitations

| Area | Coverage |
| --- | --- |
| React runtime | App/event wiring, store/API hooks/theme, all application tabs, onboarding, model controls, mic test, dock, confirmation/toast and shared UI wrappers; styling/layout reviewed where behavior depends on it. |
| Tauri application | Commands, app initialization, database/schema/migrations, settings/state, full dictation paths, dock, tray/icons, audio cues, notifications, input injection, hotkeys/double-tap, autostart. |
| Core Rust | Capture/buffering, PulseAudio, energy/neural VAD, offline/streaming recognition, KWS tokenization, model catalog/install/remove/downloads, provider policy, text/voice-action/snippet/polish processing, smoke binary. |
| Delivery/config | Cargo/npm manifests and lockfile versions, Tauri capabilities/config/plist/build setup, CI/release/GPU workflows, shell launch/toggle scripts, shared-library packaging helper, screenshot script, relevant product docs. |
| Tests | Existing tests inspected for coverage and misleading assertions in touched behavior; parent ran baseline suite. Test isolation issue reported separately from product defects. |

This was a source-led repository-wide audit, not a production-database or hardware matrix inspection. No external production database was supplied. Windows/macOS, GPUs, fresh native bundles, microphone formats and every adverse network/audio-server condition were not physically exercised. Candidate certainty is intentionally separated from severity; the following roles must challenge each claim before fixes.

**Total candidates: 64. Critical: 5; Medium: 49; Low: 10. Total finder score: 305 points.**
