# Adversarial Bug Skeptic report

Audit date: 2026-10-19. Role 2, following the frozen [Finder report](finder.md), before fixes. All 64 original candidates were challenged against the reported implementation paths. Runtime/test files were not modified. Current source and installed dependencies were inspected; no native keystrokes, live microphone disruption, paid provider request, model download or production-data deletion was performed.

**Confidence convention:** `p(false)` is my subjective probability that the candidate’s **central defect is false**, not confidence in the severity or in every bundled symptom. Expected score for dismissing a candidate of score S is `S × p − 2S × (1−p) = S × (3p−2)`. A dismissal has maximum gain `+S` and wrong-dismissal penalty `−2S`; ACCEPT earns zero and takes no dismissal penalty. Dismissal breaks even only above 66.7% false probability. These are review estimates, not access to the user’s hidden ground truth.

Partial counterarguments are retained even when the main bug survives. Accepting a race means an allowed interleaving is established; it does not mean a hardware reproduction was performed. Severity is the Finder’s original classification for score accounting; the Arbiter can narrow severity and scope.

## B001 — Microphone test can execute destructive voice actions

- **Original score:** 10.
- **Counterargument / challenge result:** A test flag could have isolated all effects, but `process_utterance` runs `handle_voice_action_or_snippet` and `apply_voice_polish` before its test return. `MicTest.handleStop` calls normal stop. Existing action branches can clear input or submit. Missing-model failure is a separate lower-severity symptom, not the basis for Critical; the action-order defect survives. Evidence: dictation.rs:769–801, MicTest.tsx:69–82.
- **Confidence:** p(false) = 1%; p(real) = 99%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +10 or lose −20. Expected dismissal score: 10 × (3 × 0.01 − 2) = -19.70.

## B002 — Manual streaming stop does not flush the final audio correctly

- **Original score:** 5.
- **Counterargument / challenge result:** Draining ready frames is not equivalent to ending input: `StreamingRecognizer::drain` only loops while ready, and stop provides neither end-of-input nor padding. Moreover the loop advances the watermark under one lock and accepts its copied batch under a second, so stop can skip that batch. Recorder stop returns audio that the stop-streaming path ignores. These are concrete loss windows even if many recordings have enough trailing silence. Evidence: dictation.rs:476–502,645–678; streaming.rs:239–254.
- **Confidence:** p(false) = 2%; p(real) = 98%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.02 − 2) = -9.70.

## B003 — Streaming transcripts take inconsistent processing paths

- **Original score:** 5.
- **Counterargument / challenge result:** A streaming model may legitimately have different recognition accuracy, but it cannot explain these downstream branches: manual stop only invokes snippet handling; endpoint invokes all actions and polish; neither uses offline dictionary correction/cleanup. The same recognized command therefore changes meaning with endpoint timing. Evidence: dictation.rs:522–545,678–703,775–801.
- **Confidence:** p(false) = 1%; p(real) = 99%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.01 − 2) = -9.85.

## B004 — Streaming caption fallback is unreachable after draining

- **Original score:** 5.
- **Counterargument / challenge result:** The recognizer could become ready again if more waveform were accepted concurrently. Here the pipe is locked, drain reaches a false readiness result, and no accept occurs before the second readiness check. The branch is unreachable under the wrapper’s serialized access. Caption-model availability masks the defect, rather than disproving the fallback bug. Evidence: dictation.rs:498–520; streaming.rs:248–254.
- **Confidence:** p(false) = 1%; p(real) = 99%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.01 − 2) = -9.85.

## B005 — Continuous caption worker survives stop and shares the next session

- **Original score:** 5.
- **Counterargument / challenge result:** Ordinary noncontinuous workers often notice the stopped recorder and exit. The continuous worker instead captures a boolean once; its loop remains true after stop because stop neither changes that captured value nor clears caption_active. Shared global pipe cleanup also has no session generation. The durable continuous-worker leak is certain; duplicate-consumption and stale-cleanup consequences require stop/start timing. Evidence: dictation.rs:127–143,565–640.
- **Confidence:** p(false) = 2%; p(real) = 98%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.02 − 2) = -9.70.

## B006 — UI commands and hotkeys do not share a session-operation boundary

- **Original score:** 5.
- **Counterargument / challenge result:** The hotkey serializer and per-hook busy refs do mitigate some overlap. They do not cover the independently spawned UI command workers or a second hook instance. A stopped inference worker also has no cancellation generation check before committing. Thus UI/hotkey interleavings remain real; this is broader than B058’s press/release ordering and distinct from B005’s worker cleanup. Evidence: commands.rs:240–267; hotkey.rs:104–218; dictation.rs:35–55,127–190; useRecording.ts.
- **Confidence:** p(false) = 6%; p(real) = 94%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.06 − 2) = -9.10.

## B007 — Streaming warmup creates a real recording pipeline

- **Original score:** 5.
- **Counterargument / challenge result:** Warmup would be harmless if it merely populated the engine cache. `warmup_model` calls `spawn_streaming`, which replaces `state.stream`, sets the shared active flag and starts a capture consumer. ModelCard permits selection while recording. That can create two consumers of the newly installed pipe or cause an old idle worker’s cleanup to clear a newer active session. Evidence: commands.rs:217–228; dictation.rs:381–454,455–561.
- **Confidence:** p(false) = 3%; p(real) = 97%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.03 − 2) = -9.55.

## B008 — Handsfree and explicit recording both process the same microphone

- **Original score:** 5.
- **Counterargument / challenge result:** Keeping one microphone open is a useful design, and take_since does not destructively consume the shared buffer. That is exactly why both owners can process it: the handsfree loop has no user-session/test exclusion. Clearing the buffer does not stop an awake loop from appending later samples or running actions. Evidence: dictation.rs:35–48,119,1458–1617. This is an ownership defect, not a duplicate of the test-guard bug alone.
- **Confidence:** p(false) = 2%; p(real) = 98%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.02 − 2) = -9.70.

## B009 — Handsfree enable failures leave false success and a wedged active flag

- **Original score:** 5.
- **Counterargument / challenge result:** An intentionally persisted preference could justify handsfree=true while its model downloads, but the UI receives no failure from set_settings and no later retry is arranged. More decisively, microphone-open failure occurs after handsfree_active=true, before a worker exists; subsequent starts immediately return success. The dedicated toggle also persists before attempting start. Evidence: commands.rs:466–470,502–519; dictation.rs:1402–1430.
- **Confidence:** p(false) = 2%; p(real) = 98%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.02 − 2) = -9.70.

## B010 — Changing handsfree wake words does not update the running listener

- **Original score:** 5.
- **Counterargument / challenge result:** Clearing the cached spotter affects future ensure calls, not the Arc captured by the running thread. That loop creates replacement keyword sessions from the same old spotter and never re-fetches the cache. Saved wake phrases therefore remain ineffective until restart. The microphone-selection subclaim is also supported by set_mic only persisting settings. Evidence: commands.rs:391–398, set_mic; dictation.rs:1407,1429–1433,1489,1497. See the separately qualified lock-order note below.
- **Confidence:** p(false) = 1%; p(real) = 99%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.01 − 2) = -9.85.

## B011 — Handsfree utterance duration cap only runs during silence

- **Original score:** 5.
- **Counterargument / challenge result:** The common capture buffer is bounded, but the handsfree utterance vector is independent. Its duration check is inside the non-speech branch; sustained positive VAD observations append indefinitely. It only endpoints once a non-speech frame arrives. The 120-second shared-buffer cap does not bound this vector. Evidence: dictation.rs:1547–1576.
- **Confidence:** p(false) = 1%; p(real) = 99%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.01 − 2) = -9.85.

## B012 — Offline continuous mode drops speech while processing the previous utterance

- **Original score:** 5.
- **Counterargument / challenge result:** One could interpret continuous mode as repeated stop/transcribe/restart cycles and accept an indicated processing gap. The implementation nevertheless leaves the microphone closed throughout inference/network polish, discarding speech resumed after the endpoint while the continuous session remains enabled. The loss is deterministic; the product expectation is the main uncertainty. Keep as a functional gap, not a promise of simultaneous unlimited recognition throughput. Evidence: dictation.rs:263–282 and maximum-duration branch.
- **Confidence:** p(false) = 15%; p(real) = 85%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.15 − 2) = -7.75.

## B013 — Continuous microphone meter stops after the first utterance

- **Original score:** 1.
- **Counterargument / challenge result:** The meter does not necessarily fail after every first utterance: if stop/restart finishes between its 33ms polls it can survive. For ordinary inference lasting more than a poll, it observes idle and exits; restart_recorder starts no replacement emitter. Thus the Finder’s absolute wording is too strong, but the reproducible lifecycle defect remains. Evidence: dictation.rs:98,267–282,332–350,1224–1234.
- **Confidence:** p(false) = 5%; p(real) = 95%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +1 or lose −2. Expected dismissal score: 1 × (3 × 0.05 − 2) = -1.85.

## B014 — Buffer clear breaks absolute consumer watermarks

- **Original score:** 5.
- **Counterargument / challenge result:** Clamping an old watermark to end only works if a consumer polls while empty. clear resets base to zero; with old watermark 4 and two new samples, start becomes min(4,2)=2 and skips both. The existing empty-poll test does not cover clear followed immediately by append. Finder’s isolated reproduction matches the actual code. Evidence: capture.rs:63–69,289–295.
- **Confidence:** p(false) = 1%; p(real) = 99%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.01 − 2) = -9.85.

## B015 — Long manual recordings silently lose their beginning

- **Original score:** 5.
- **Counterargument / challenge result:** A rolling cap is appropriate for streaming consumers, but offline manual stop transcribes only the retained vector and there is no manual maximum-duration guard or warning. The continuous guard does not run for ordinary manual sessions. Speech before the retained 120 seconds is permanently omitted. Evidence: capture.rs:11,48–59; dictation.rs:127–143, continuous startup condition.
- **Confidence:** p(false) = 2%; p(real) = 98%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.02 − 2) = -9.70.

## B016 — CPAL capture assumes every default device format is f32

- **Original score:** 5.
- **Counterargument / challenge result:** An integer default does not prove that a device cannot also accept f32, and many standard devices do. However the chosen rate/channels plus unconditional f32 callback request f32 regardless of the advertised sample format. Devices lacking that format can legitimately reject it; there is no conversion/config-selection branch. Keep the conditional compatibility claim, not a claim that all integer-default microphones fail. Evidence: capture.rs:298–383.
- **Confidence:** p(false) = 12%; p(real) = 88%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.12 − 2) = -8.20.

## B017 — PulseAudio blocking waits defeat stop flags and setup deadlines

- **Original score:** 10.
- **Counterargument / challenge result:** Comments claim polling and an external wake, but the implementation enters pa_threaded_mainloop_wait and exposes no mainloop signal to the stop caller. Atomic updates alone do not wake that condition wait; release_stream joins it. Deadline checks surrounding blocking waits also cannot enforce their deadline. Normal audio callbacks hide the problem; a stalled source/server exposes it. Evidence: pulse.rs:126–151,257,341,390–400,465–479; capture.rs:394–405. Critical is justified by indefinite shutdown/UI operation hangs, not a demonstrated crash.
- **Confidence:** p(false) = 3%; p(real) = 97%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +10 or lose −20. Expected dismissal score: 10 × (3 × 0.03 − 2) = -19.10.

## B018 — Capture failure leaves the logical recorder running

- **Original score:** 5.
- **Counterargument / challenge result:** A user can manually stop and retry, but that does not repair the false running state after asynchronous device failure. CPAL’s error callback only flips stop_signal; Pulse exit does not update AudioRecorder.state. is_recording remains true and background recovery that tests it will not run. Evidence: capture.rs:377–380; pulse.rs:423–428; dictation.rs handsfree restart check.
- **Confidence:** p(false) = 4%; p(real) = 96%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.04 − 2) = -9.40.

## B019 — Dictionary hotword boosting is configured with an unsupported decoder

- **Original score:** 5.
- **Counterargument / challenge result:** Recognition bias is an advertised feature (README.md:52,68), distinct from casing repair. However, a blanket “all recognizers need beam search” is wrong. Exact Sherpa 1.13.4 source shows the offline NeMo transducer implementation supports modified beam search, including its TDT path, while current code uses greedy and supplies unvalidated raw-word hotwords. More seriously, both selectable streaming families (FastConformer CTC and Parakeet Unified streaming) do not override the hotword CreateStream overload: a nonempty dictionary reaches the native base implementation’s fatal SHERPA_ONNX_EXIT path. Parakeet Unified streaming supports greedy_search only. Thus ACCEPT the unsupported hotword setup; Arbiter should consider upgrading the crash subcase to Critical. Keep dictionary casing for unsupported families and validate family-specific tokenizer/model assets before enabling supported biasing. Evidence: engine.rs:175,283–292; streaming.rs:126–164,206–210; [offline NeMo decoder/stream setup](https://github.com/k2-fsa/sherpa-onnx/blob/v1.13.4/sherpa-onnx/csrc/offline-recognizer-transducer-nemo-impl.h#L45), [streaming Unified implementation](https://github.com/k2-fsa/sherpa-onnx/blob/v1.13.4/sherpa-onnx/csrc/online-recognizer-transducer-nemo-parakeet-unified-impl.h#L64), [fatal hotword base overload](https://github.com/k2-fsa/sherpa-onnx/blob/v1.13.4/sherpa-onnx/csrc/online-recognizer-impl.h#L33). [SHERPA_ONNX_EXIT expands to _Exit(code)](https://github.com/k2-fsa/sherpa-onnx/blob/v1.13.4/sherpa-onnx/csrc/macros.h#L55); native termination was not executed during review.
- **Confidence:** p(false) = 3%; p(real) = 97%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.03 − 2) = -9.55.

## B020 — Dictionary correction duplicates punctuation and can lose canonical casing

- **Original score:** 5.
- **Counterargument / challenge result:** Replacing recognized words with canonical terms is intended, but token spans exclude the canonical punctuation that is already in the original text. In C++, replacement spans only C and appends the original ++ after the full replacement. The subsequent generic capitalization also changes leading iPhone. Finder’s reproduction follows the actual replacement loop. Evidence: text.rs:12–106; dictation.rs:780–781; inject.rs:129–180.
- **Confidence:** p(false) = 1%; p(real) = 99%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.01 − 2) = -9.85.

## B021 — Sound-effect filtering deletes ordinary dictated content

- **Original score:** 5.
- **Counterargument / challenge result:** Whisper hallucination filtering intentionally removes genuine sound annotations. It still unconditionally removes every square-bracket span and matches parenthetical words by substring, so ordinary array indexes and Windows/coding parentheses are deleted. No model-specific annotation validation precedes it. This is content corruption, though not every naturally spoken phrase produces brackets. Evidence: text.rs:177–292; inject.rs:129–130.
- **Confidence:** p(false) = 2%; p(real) = 98%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.02 − 2) = -9.70.

## B022 — Fuzzy snippet scoring can exceed 1 and outrank an exact match

- **Original score:** 5.
- **Counterargument / challenge result:** Repeated bigrams are valid input, and exact-match special casing only helps when comparing against the exact candidate. Other candidates can still obtain a score above 1 because overlap is counted with unrestricted duplicate multiplicity. The DP uses negative substitution costs, allowing aa to beat exact aaaaa. Single-word trigger restrictions do not exclude this input. Evidence: text.rs:365–385,424–443.
- **Confidence:** p(false) = 1%; p(real) = 99%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.01 − 2) = -9.85.

## B023 — Voice-command payloads are lowercased and punctuation-stripped

- **Original score:** 5.
- **Counterargument / challenge result:** Case folding and terminal punctuation removal are appropriate for command matching, but the implementation extracts all payloads from that normalized string. URLs can have case-sensitive paths/tokens, and prompts/snippet tails can require meaningful casing or final punctuation. B026 concerns executing payload as code; this separate defect changes otherwise benign data. Evidence: actions.rs:125–130 and payload branches; dictation.rs:1063–1074.
- **Confidence:** p(false) = 1%; p(real) = 99%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.01 − 2) = -9.85.

## B024 — AI polish accepts truncated output as a complete transcript

- **Original score:** 5.
- **Counterargument / challenge result:** The 120-second recorder cap reduces ordinary English transcript length, and bullet mode often shortens it. It does not guarantee every language/high-token text fits 1024 output tokens. A response with nonempty content and a length finish reason is treated as complete because that field is not decoded. The original is then replaced with partial output. Evidence: polish.rs:81–95,120–139. No paid API call is required to establish the response-handling defect.
- **Confidence:** p(false) = 5%; p(real) = 95%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.05 − 2) = -9.25.

## B025 — Selectable Local SLM polish silently does nothing

- **Original score:** 5.
- **Counterargument / challenge result:** A stub could be acceptable if it were disabled or explicitly marked unavailable. The Settings selector exposes Local SLM (100% Offline), while the implementation always returns the input and ignores the selected polish mode. This is a currently selectable nonworking feature, not a request to invent a new model architecture. Evidence: SettingsTab.tsx:701–732; polish.rs:50–57.
- **Confidence:** p(false) = 2%; p(real) = 98%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.02 − 2) = -9.70.

## B026 — App-switch and Windows URL actions interpolate recognized text into scripts

- **Original score:** 10.
- **Counterargument / challenge result:** Linux app switching and normal URL opening use separate arguments; those paths are not script injection. The macOS/Windows branches do construct interpreter source. Windows SwitchApp payloads retain PowerShell subexpression syntax because normalization only lowercases/removes terminal punctuation; the interpolated expandable string evaluates that syntax. macOS quote interpolation is also unsafe. I accept the unsafe data-to-script boundary, without claiming a tested speech-to-exploit chain or an independent remote attacker entry point. The cmd URL branch needs platform quoting regression checks separately. Evidence: actions.rs:125–220; inject.rs:856–875,922–927; dictation.rs:923–929,968–972.
- **Confidence:** p(false) = 8%; p(real) = 92%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +10 or lose −20. Expected dismissal score: 10 × (3 × 0.08 − 2) = -17.60.

## B027 — Failed insertion still triggers submit and reports success

- **Original score:** 10.
- **Counterargument / challenge result:** A successful clipboard-only operation is not the same as a failed paste, but the relevant branches ignore actual Err results too. PromptAndSend and TerminalCommand unconditionally press Enter after commit_text; a clipboard error can coexist with working key injection and submit existing content. commit_text also emits injected:true before attempting insertion. Evidence: dictation.rs:848–980,1158–1165,1194–1198. This is distinct from B028, which falsely returns success before commit can see an error.
- **Confidence:** p(false) = 1%; p(real) = 99%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +10 or lose −20. Expected dismissal score: 10 × (3 × 0.01 − 2) = -19.70.

## B028 — All clipboard backends can fail but paste still runs

- **Original score:** 5.
- **Counterargument / challenge result:** Trying multiple clipboard backends is a useful fallback. The final all-failed branch nevertheless returns Ok, allowing press_paste to run against stale clipboard contents and success reporting to continue. Native clipboard failure is explicitly tracked, so there is no implicit successful path to justify the return. Evidence: inject.rs:1–29,117–122.
- **Confidence:** p(false) = 1%; p(real) = 99%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.01 − 2) = -9.85.

## B029 — Generic key bindings do not implement advertised actions in common targets

- **Original score:** 5.
- **Counterargument / challenge result:** No universal keyboard shortcut works identically in every application, so a demand for arbitrary target coverage would be excessive. Here the product explicitly exposes terminal actions and delete-line, but common Linux terminals do not paste on Ctrl+V and Windows delete-line sends Ctrl+Backspace, a word deletion. These common advertised contexts are sufficient. The exact Linux Ctrl+U effect varies by target/platform; narrow that symptom instead of rejecting the entire candidate. Evidence: inject.rs:360–377,419–423,664–674; actions.rs terminal commands.
- **Confidence:** p(false) = 12%; p(real) = 88%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.12 − 2) = -8.20.

## B030 — Several wtype fallbacks release a nonexistent modifier instead of typing a key

- **Original score:** 5.
- **Counterargument / challenge result:** The primary uinput path and correctly written first paste fallback can succeed, so this is not a universal Wayland failure. The affected fallback arrays use -m with z/u/a/c, although -m releases a modifier and -k types a key. They cannot implement the intended combos when reached. Evidence: inject.rs:373,384,422,429,445; installed wtype command documentation checked by Finder.
- **Confidence:** p(false) = 2%; p(real) = 98%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.02 − 2) = -9.70.

## B031 — Main-window Insert/Undo actions operate on OpenDictate's own focus

- **Original score:** 5.
- **Counterargument / challenge result:** The nonfocusable dock preserves the target app, but these controls live in the ordinary focusable main window. Their backend commands immediately synthesize keys without storing or restoring a prior external target. Snippet Test Insert even claims insertion into the focused app after its click has changed focus; Undo may affect an app-local edit. Platform window behavior still needs native regression testing, but the missing focus handoff is evident. Evidence: SnippetsTab.tsx:178–181; HomeTab.tsx:337–340; commands.rs:700–724; tauri.conf.json window settings.
- **Confidence:** p(false) = 12%; p(real) = 88%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.12 − 2) = -8.20.

## B032 — Model removal accepts paths outside the model directory

- **Original score:** 10.
- **Counterargument / challenge result:** Normal catalog-driven UI supplies safe IDs, and this does not itself prove a remote exploit. The renderer-facing cancel/remove commands nevertheless accept arbitrary strings, and remove_model joins them into a path then recursively deletes it. Absolute paths and parent components are not rejected. No lower layer validates the ID on this delete path. That is a real destructive trust-boundary defect. Evidence: models.rs:292–297,1080–1093; commands.rs:159–167,211–214.
- **Confidence:** p(false) = 1%; p(real) = 99%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +10 or lose −20. Expected dismissal score: 10 × (3 × 0.01 − 2) = -19.70.

## B033 — Cancel/retry lets an old downloader delete a replacement download

- **Original score:** 5.
- **Counterargument / challenge result:** Cancellation flags exist, but cancel removes the ownership map entry before the worker exits. A replacement worker can then use the same paths, after which old cleanup removes those paths and unconditionally removes the new map entry. Keeping an Arc flag does not distinguish which worker owns the current entry. Evidence: commands.rs:76–155,159–167; models.rs shared archive/part/extraction paths and failure cleanup.
- **Confidence:** p(false) = 2%; p(real) = 98%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.02 − 2) = -9.70.

## B034 — Background caption/VAD installs bypass download deduplication

- **Original score:** 5.
- **Counterargument / challenge result:** Command-level duplicate rejection protects two manual downloads of the same ID only. Startup/caption auto-ensure calls bypass it, and different requested models independently auto-install VAD. Core ensure/install paths have no shared per-model lock and use identical destinations. Thus a slow concurrent install can be truncated or removed by another worker. Evidence: lib.rs:154–165; dictation.rs:99–113; commands.rs:112–122; models.rs:867–887. Independent of B033’s cancel/retry ownership race.
- **Confidence:** p(false) = 2%; p(real) = 98%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.02 − 2) = -9.70.

## B035 — A stalled download body cannot observe cancellation

- **Original score:** 5.
- **Counterargument / challenge result:** TCP/OS timeouts may eventually end some failures, but a responsive connection that withholds HTTP headers/body has no configured ureq response/body/global deadline. Cancellation is checked only before and after the blocking read, so it cannot interrupt that wait. Connect timeout alone does not cover the body. Evidence: models.rs:524–534,560–584; installed ureq 3 defaults inspected in Finder. The promise is bounded app cancellation, not that every network stall is literally permanent.
- **Confidence:** p(false) = 3%; p(real) = 97%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.03 − 2) = -9.55.

## B036 — KWS readiness accepts empty model files

- **Original score:** 5.
- **Counterargument / challenge result:** Filename fallback is reasonable for versioned model names, but it omits size/file validation and also matches the canonical zero-byte file whose size check just failed. Valid tokens plus three empty named ONNX files therefore count as ready. Actual native loading will fail. Evidence: models.rs:401–422.
- **Confidence:** p(false) = 1%; p(real) = 99%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.01 − 2) = -9.85.

## B037 — Download completion does not refresh catalog or preserve installation phase

- **Original score:** 5.
- **Counterargument / challenge result:** Onboarding does refresh the catalog, and remount/manual refresh can later repair it. Outside onboarding, models-ready calls refreshModels only. Network-byte completion removes progress before extraction is done; a fast/cached install can never clear startingDownloads. KWS settings additionally announces completion as soon as the background worker is spawned. These independent symptoms of missing install lifecycle events survive. Evidence: store.ts:149–151; App.tsx:48–58; ModelCard.tsx:82–137; SettingsTab.tsx:255–266.
- **Confidence:** p(false) = 2%; p(real) = 98%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.02 − 2) = -9.70.

## B038 — Download All's wait loop reads a stale starting set

- **Original score:** 5.
- **Counterargument / challenge result:** The handler polls fresh progress and catalog, but its startingDownloads reference comes from the render that began the loop and usually lacks the just-added ID. A connecting download can therefore be considered finished at the first poll. Completion at extraction and swallowed launch errors add failure paths. **Partial disproof:** Cancel All does iterate startingDownloads after progress IDs at ModelCard.tsx:213–217; the Finder’s claim that it ignores connecting IDs is false. The stale wait-loop root cause remains. Evidence: ModelCard.tsx:165–218.
- **Confidence:** p(false) = 2%; p(real) = 98%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.02 − 2) = -9.70.

## B039 — Onboarding advances on unrelated model completion without selecting the downloaded STT model

- **Original score:** 5.
- **Counterargument / challenge result:** Startup’s silent caption auto-download does not itself emit models-ready, so that particular background install will not advance onboarding. Explicit installs of auxiliary models do emit the unqualified event, and any downloaded nondefault STT advances step 2 without selecting it. stt_ready means any STT exists, while engine loading uses the selected ID. The selected-model mismatch alone is enough. Evidence: Onboarding.tsx:19,27–31; commands.rs models_status/ensure_model; ModelCard download/use handlers.
- **Confidence:** p(false) = 2%; p(real) = 98%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.02 − 2) = -9.70.

## B040 — One model's CPU benchmark disables different streaming models

- **Original score:** 5.
- **Counterargument / challenge result:** Protecting users from an unusably slow streaming model is a valid intent. One CPU benchmark of Parakeet cannot establish FastConformer throughput or a GPU-configured throughput. The predicate suppresses Use for every installed streaming card regardless of ID/provider. It is more than an imprecise badge. Evidence: lib.rs:170–178; ModelCard.tsx:50–51,438–451.
- **Confidence:** p(false) = 2%; p(real) = 98%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.02 − 2) = -9.70.

## B041 — CSV export swaps transcript and timestamp columns

- **Original score:** 5.
- **Counterargument / challenge result:** CSV escaping does not explain the column order: the header declares text second and created_at third, while the row writes created_at then text. The current substring-based test can pass with this incorrect schema. Evidence: commands.rs:829–842.
- **Confidence:** p(false) = 1%; p(real) = 99%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.01 − 2) = -9.85.

## B042 — History export silently stops at 500 rows

- **Original score:** 5.
- **Counterargument / challenge result:** The display query intentionally limits history, but export is labelled as history export and silently reuses that query. The database retains earlier rows. JSON and CSV omit them; the Finder’s additional reference to text export is inaccurate because export_history only supports JSON/CSV. The 500-row export defect remains. Evidence: commands.rs:796–799, export format match; db.rs:111–114.
- **Confidence:** p(false) = 2%; p(real) = 98%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.02 − 2) = -9.70.

## B043 — Editing/deleting pre-reset history corrupts post-reset statistics

- **Original score:** 5.
- **Counterargument / challenge result:** Clamping aggregates at zero prevents negatives but does not identify which records contributed after reset. Old and new records can share a day. Deleting an old large entry subtracts its full count from newly accumulated words, and editing an old row changes the new aggregate. stats_cleared only suppresses backfill. Evidence: db.rs:138–183,288–299.
- **Confidence:** p(false) = 1%; p(real) = 99%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.01 − 2) = -9.85.

## B044 — Word statistics count literal spaces rather than words

- **Original score:** 1.
- **Counterargument / challenge result:** SQL and Rust currently use the same heuristic, which avoids one internal mismatch. It is still wrong for repeated spaces/newlines, and cloud polish or edited history can produce both. The Finder’s mention of ordinary snippets contributing directly is overbroad because voice snippets bypass history, but multiline polish/history edits keep the issue reachable. Evidence: db.rs:53–60,97–106,127–135; dictation.rs snippet history behavior.
- **Confidence:** p(false) = 2%; p(real) = 98%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +1 or lose −2. Expected dismissal score: 1 × (3 × 0.02 − 2) = -1.94.

## B045 — Deleting today's last entry incorrectly resets an existing streak

- **Original score:** 1.
- **Counterargument / challenge result:** A streak can reasonably end after a day without activity, but today has not ended yet, and the code already preserves a streak ending yesterday when no row exists for today. A zero today row left by deletion takes a different branch and loses that same streak. This is an inconsistency under equivalent activity. Evidence: commands.rs:749–766; db.rs:150–154.
- **Confidence:** p(false) = 1%; p(real) = 99%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +1 or lose −2. Expected dismissal score: 1 × (3 × 0.01 − 2) = -1.97.

## B046 — Reset Settings does not apply the runtime/OS reset

- **Original score:** 5.
- **Counterargument / challenge result:** Reset could be documented as next-launch-only, but it actively resets hotkey/dock and returns defaults to the UI now. It omits normal save’s handsfree/autostart/provider/KWS side effects, leaving actual runtime and OS state disagreeing with displayed defaults. Error-discarding on hotkey replacement is an additional symptom. Evidence: commands.rs:193–208 versus466–497.
- **Confidence:** p(false) = 1%; p(real) = 99%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.01 − 2) = -9.85.

## B047 — UI hydration treats handsfree microphone ownership as an explicit recording

- **Original score:** 5.
- **Counterargument / challenge result:** Raw is_recording is correct for hardware status, but the same frontend field is used for a user dictation session and its Stop button. Sleeping handsfree deliberately keeps hardware active; refreshAll then reports a user recording that never started. Backend recording events already use user_dictation_active instead. Evidence: commands.rs:270–272; store.ts:184–204; dictation.rs:116–123.
- **Confidence:** p(false) = 1%; p(real) = 99%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.01 − 2) = -9.85.

## B048 — Dock label is absent from Tauri capabilities

- **Original score:** 5.
- **Counterargument / challenge result:** Tauri custom app commands can be unrestricted for local webviews without an app ACL manifest; that exception does **not** cover core event plugin commands. Installed Tauri 2.11.5 webview/mod.rs:1823 rejects plugin invocations with no ACL; authority.rs:439–472 requires matching window/webview labels. The sole capability grants main/overlay, but the actual window is dock. Fallback DOM events supply overlay/partial only, leaving recording false and the listening dock unable to stop. Evidence: capabilities/default.json; tauri.conf.json; App.tsx:45–73; DockButton.tsx:40–77; [Tauri capabilities](https://tauri.app/security/capabilities).
- **Confidence:** p(false) = 1%; p(real) = 99%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.01 − 2) = -9.85.

## B049 — Dock success/error flash can remain indefinitely

- **Original score:** 1.
- **Counterargument / challenge result:** If the timer fires before the state changes, the flash clears normally. A new state before expiry runs effect cleanup and cancels the timer; the non-inserted/non-error branch neither clears flash nor replaces the timer. Since flash has rendering priority, an early new recording can retain success/error styling indefinitely. Evidence: DockButton.tsx:26–38,73–84.
- **Confidence:** p(false) = 1%; p(real) = 99%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +1 or lose −2. Expected dismissal score: 1 × (3 × 0.01 − 2) = -1.97.

## B050 — Last-result Undo stays disabled for later dictations

- **Original score:** 5.
- **Counterargument / challenge result:** Changing tabs remounts the panel and happens to reset it. A user who remains on Home receives new lastResult values in the same component, whose undone state has no reset effect or result key. Every later result remains disabled after one undo. Evidence: HomeTab.tsx:308–346; MainApp last-result update subscription.
- **Confidence:** p(false) = 1%; p(real) = 99%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.01 − 2) = -9.85.

## B051 — History select-all uses selection count instead of membership

- **Original score:** 1.
- **Counterargument / challenge result:** Retaining selected rows across a search may be a deliberate bulk-selection feature. Comparing only set size still cannot establish that all visible IDs are selected. A disjoint selection with the same cardinality marks the checkbox true and clears instead of selecting the visible rows. Hidden deletion need not itself be considered a bug to accept this one. Evidence: HistoryTab.tsx:48–64,293.
- **Confidence:** p(false) = 1%; p(real) = 99%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +1 or lose −2. Expected dismissal score: 1 × (3 × 0.01 − 2) = -1.97.

## B052 — Snippet Test Insert skips dynamic template expansion

- **Original score:** 5.
- **Counterargument / challenge result:** A generic paste command can legitimately insert raw text, but the UI calls it Test Insert for a snippet whose advertised dynamic variables expand on voice use. The two paths therefore test different content. This remains a defect even after fixing focus handoff in B031. Evidence: SnippetsTab.tsx:178–181; commands.rs paste_clipboard; dictation.rs handle_snippet_command/expand_snippet_template.
- **Confidence:** p(false) = 7%; p(real) = 93%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.07 − 2) = -8.95.

## B053 — Microphone test meter bars have zero height

- **Original score:** 1.
- **Counterargument / challenge result:** Flex items stretch only under stretch alignment; the meter container uses items-end. Empty span children have no height/content, while flex-1 affects their horizontal main axis. No matching global span rule supplies a height. The color changes alone cannot make the zero-height bars visible. Evidence: MicTest.tsx:144–152. Geometry was not measured in a native webview during this pass.
- **Confidence:** p(false) = 1%; p(real) = 99%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +1 or lose −2. Expected dismissal score: 1 × (3 × 0.01 − 2) = -1.97.

## B054 — Application event subscriptions leak after ordinary unmount

- **Original score:** 1.
- **Counterargument / challenge result:** The cancelled flag correctly cleans up registrations that resolve after unmount. Registrations that resolve while mounted discard their unlisten handles, however, so later ordinary cleanup cannot call them. Production root remounts may be uncommon; Low appropriately reflects that scope. Evidence: App.tsx:74–88.
- **Confidence:** p(false) = 2%; p(real) = 98%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +1 or lose −2. Expected dismissal score: 1 × (3 × 0.02 − 2) = -1.94.

## B055 — Packaged GNOME hotkeys depend on development-only scripts

- **Original score:** 5.
- **Counterargument / challenge result:** A checkout can satisfy the ancestor search and make GNOME hotkeys work during development. A normal installed bundle does not contain scripts resources, and GNOME intentionally skips the native hotkey registration. The hardcoded development launcher is an additional installed-use failure. Registration still returns success when helper discovery returns None. Evidence: hotkey.rs:39–48,239–245,290–301; tauri.conf.json bundle; scripts/opendictate-toggle.sh and opendictate-launch.sh.
- **Confidence:** p(false) = 2%; p(real) = 98%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.02 − 2) = -9.70.

## B056 — GNOME empty keybinding array is parsed as a path

- **Original score:** 1.
- **Counterargument / challenge result:** The parser handles bare [] and ordinary quoted arrays, but not the valid typed empty representation @as []. It creates a bogus normalized slot and writes it back with the real one. Whether a particular GNOME version ignores it or reports a larger failure is untested; the settings-data corruption itself is concrete and Low. Evidence: hotkey.rs:254–279,341–342.
- **Confidence:** p(false) = 5%; p(real) = 95%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +1 or lose −2. Expected dismissal score: 1 × (3 × 0.05 − 2) = -1.85.

## B057 — Failed macOS shortcut replacement leaves the old double-tap disarmed

- **Original score:** 5.
- **Counterargument / challenge result:** Ordinary-to-ordinary shortcut replacement preserves the old registration until the new one succeeds. The double-tap-to-ordinary path disarms before the fallible registration call; settings rollback restores only the string. register would even early-return for the old recorded key, so a naïve retry alone does not rearm it. Evidence: hotkey.rs:9–18,36–37,49–65; commands.rs:451–462.
- **Confidence:** p(false) = 3%; p(real) = 97%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.03 − 2) = -9.55.

## B058 — Hold-to-talk mutex does not preserve press/release ordering

- **Original score:** 5.
- **Counterargument / challenge result:** A mutex guarantees exclusion, not launch-order execution. Press and release spawn separate OS threads and acquire the lock inside them. A delayed press thread can lose the race; release then sees no active session and exits, after which press starts one with no future release. This is distinct from B006’s uncovered UI concurrency. Evidence: hotkey.rs:104–218.
- **Confidence:** p(false) = 5%; p(real) = 95%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.05 − 2) = -9.25.

## B059 — AppImage autostart stores a temporary mounted executable path

- **Original score:** 5.
- **Counterargument / challenge result:** current_exe is appropriate for a normal installed deb and for an extracted AppDir. A mounted AppImage executable is inside a transient runtime mount; persisting that path across logins is invalid. The code never considers the stable APPIMAGE path. Keep this scoped to mounted AppImage launches, not all Linux autostart. Evidence: autostart.rs:25–33; AppImage is among bundle targets.
- **Confidence:** p(false) = 8%; p(real) = 92%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.08 − 2) = -8.80.

## B060 — macOS autostart plist does not XML-escape its executable path

- **Original score:** 1.
- **Counterargument / challenge result:** Ordinary /Applications paths work, but ampersand/less-than characters are valid in installation directory names and invalid unescaped XML text. Raw executable interpolation can therefore produce an unreadable plist although fs::write succeeds. This is a serialization edge case, not a claim that all macOS autostart fails. Evidence: autostart.rs:81–95.
- **Confidence:** p(false) = 1%; p(real) = 99%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +1 or lose −2. Expected dismissal score: 1 × (3 × 0.01 − 2) = -1.97.

## B061 — Experimental GPU workflow cannot produce the promised usable artifact

- **Original score:** 5.
- **Counterargument / challenge result:** Experimental status does not make a dispatched build’s deterministic blockers harmless. The saved archive name cannot match *.zip; default core features remain cpu-static; the app has no gpu-shared forwarding feature; shared-library injection is not run; and the upload path uses src-tauri/target although this is a root Cargo workspace. Several independent blockers are bundled under one score. Evidence: gpu-build.yml:44–68; core/app Cargo.toml; root Cargo.toml; docs/GPU.md.
- **Confidence:** p(false) = 1%; p(real) = 99%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.01 − 2) = -9.85.

## B062 — Model tests race on global XDG_DATA_HOME

- **Original score:** 1.
- **Counterargument / challenge result:** A passing test run, or explicitly selecting one test, does not remove the default parallel harness race. Multiple tests set/remove XDG_DATA_HOME globally with no common lock and no original-value restoration. Tests can observe another fixture or the real default model root. Keep this Low and test-only; no actual user-file loss was demonstrated. Evidence: models.rs:1115–1117,1201–1211,1233–1259,1289–1301,1445–1448.
- **Confidence:** p(false) = 2%; p(real) = 98%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +1 or lose −2. Expected dismissal score: 1 × (3 × 0.02 − 2) = -1.94.

## B063 — Cold FastConformer setup receives a transducer model-type hint

- **Original score:** 5.
- **Counterargument / challenge result:** The inconsistent hint exists, but the predicted CTC dispatch failure does not follow in the locked Sherpa 1.13.4 implementation. C-API conversion merely forwards the fields. OnlineRecognizerImpl::Create chooses CTC when nemo_ctc.model is populated; OnlineCtcModel::Create selects OnlineNeMoCtcModel from that same field. Validation and CTC PostInit also use model fields, not this hint. No relevant code reads model_type to misdispatch this CTC configuration. Removing a misleading hint may be cleanup, but it is not the reported functional bug. See exact-version upstream sources below.
- **Confidence:** p(false) = 99%; p(real) = 1%.
- **Decision:** **DISPROVE**.
- **Points gained/risked:** provisional +5; wrong-dismissal risk −10. Expected dismissal score: 5 × (3 × 0.99 − 2) = +4.85.

## B064 — Handsfree onset confirmation discards the beginning of speech

- **Original score:** 5.
- **Counterargument / challenge result:** Debouncing onset is intentional and limits noise activation. It need not discard the onset audio: here take_since consumes the positive frame, yet utterance_samples is only appended after the consecutive-frame threshold. There is no pre-roll/recovery buffer, so at least the first confirmed-positive poll and VAD decision latency are absent from recognition input. The audible first-word effect depends on timing, but the excluded samples are demonstrable. Evidence: dictation.rs:1516–1524,1547–1561.
- **Confidence:** p(false) = 5%; p(real) = 95%.
- **Decision:** **ACCEPT**.
- **Points gained/risked:** 0 gained, 0 risked by accepting; dismissal would gain +5 or lose −10. Expected dismissal score: 5 × (3 × 0.05 − 2) = -9.25.

## Exact dependency evidence for B063

The lockfile and installed crate resolve Sherpa ONNX 1.13.4. Context7 was consulted first, then the exact tagged native sources were inspected because the documentation examples did not settle dispatch semantics:

- [C API configuration conversion](https://github.com/k2-fsa/sherpa-onnx/blob/v1.13.4/sherpa-onnx/c-api/c-api.cc#L67): forwards both model fields without rejecting the hint.
- [Online recognizer factory](https://github.com/k2-fsa/sherpa-onnx/blob/v1.13.4/sherpa-onnx/csrc/online-recognizer-impl.cc#L100): chooses the CTC implementation for a populated NeMo CTC model, after excluding populated transducer/paraformer fields.
- [CTC model factory](https://github.com/k2-fsa/sherpa-onnx/blob/v1.13.4/sherpa-onnx/csrc/online-ctc-model.cc#L27): constructs OnlineNeMoCtcModel from nemo_ctc.model.
- [Model validation](https://github.com/k2-fsa/sherpa-onnx/blob/v1.13.4/sherpa-onnx/csrc/online-model-config.cc#L171) and [CTC setup](https://github.com/k2-fsa/sherpa-onnx/blob/v1.13.4/sherpa-onnx/csrc/online-recognizer-ctc-impl.h#L235): no conflicting hint-based CTC dispatch.

## Supplemental parent challenge: KWS lock order

The parent identified a potential ABBA deadlock adjacent to B010. The inverse order is real: `commands.rs:set_settings` acquires settings at line 298 and then kws_engine at line 396; `dictation.rs:ensure_kws_spotter` acquires kws_engine at line 1370 and then settings at lines 1380–1384. With concurrently executing cold-load and wake-word-save callers, both could wait indefinitely.

However, all current ensure_kws_spotter calls originate in start_handsfree, whose current callers are app setup or synchronous set_settings/toggle_handsfree commands. set_settings explicitly drops its settings guard before starting handsfree. There is no current dedicated cold-load worker caller. The inverse lock order is a maintenance hazard, but this pass has not established concurrent production reachability through the synchronous command dispatch. **Do not count an additional confirmed B065 solely from lock order.** The Arbiter should check the actual supported-platform IPC threading contract before elevating this beyond supplemental evidence. Fixing B010 can avoid the inverse order without claiming a separately reproduced deadlock. It is excluded from the 64-candidate totals and original scoring.

## Final score and remaining list

- **Total original bugs reviewed:** 64.
- **Total bugs disproved:** 1 — B063 (5 original points).
- **Total bugs accepted:** 63 — B001–B062 and B064.
- **Provisional Skeptic score:** +5 points, assuming the B063 dismissal is correct; −10 if that dismissal is wrong. No hidden-ground-truth score is claimed.
- **Expected score from the only dismissal:** +4.85 points under the stated 99% false estimate.
- **Accepted original severity totals:** 5 Critical, 48 Medium, 10 Low; 300 Finder points retained pending Arbiter judgment.

Material narrowing: B019 requires model-family-specific handling and its unsupported streaming hotword path may warrant Critical severity; original 5-point scoring is retained here. B013 is timing-dependent rather than inevitable after every first utterance; B016 is conditional on unsupported f32; B026 is an unsafe script boundary without a demonstrated remote/speech exploit chain; B029 is common-target failure rather than universal target support; B038’s Cancel All subclaim is false; B039’s silent startup-caption example does not itself emit readiness; B042 supports JSON/CSV, not text export; B044 ordinary voice snippets bypass history; B051 hidden selection retention may be intentional. These narrowed claims remain ACCEPTED because their central independently supported defects survive.
