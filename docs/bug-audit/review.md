# Independent implementation review

The Finder, Skeptic, and Arbiter completed their sequential audit before fixes.
A separate reviewer then inspected the integrated patch in two passes. These
six corrections refine existing confirmed findings; they are not added to the
64-candidate scoring totals.

| Review issue | Correction and evidence |
|---|---|
| A provisional GTK clipboard path could not verify PRIMARY ownership. | Removed that path. Clipboard backends must report successful writes before insertion; failures propagate and prevent the paste. Policy regressions pass. |
| One global delete-line shortcut still broke either text fields or shells. | Added explicit Text fields and Shell (Bash, Zsh) settings, with profile-specific paste and deletion chords. Native chord tests and the real settings interaction test pass. Custom/vi key maps remain outside the supported shell profile. |
| Copied/failed transcript results could expose Undo for an older insertion. | Preserve insertion/copy/error outcomes through events and state. Native stale insertion state is cleared; UI requires an explicit successful insertion for the current result. Successful-A → copied/failed-B regressions pass. |
| The action parser preserved payloads, but upstream transcript preparation still normalized them. | Production preparation now classifies actions/snippets from original text before ordinary prose processing. URL path/query, prompt literal words, and mixed-case snippet suffix tests were reproduced red, then green. |
| A persisted unavailable Local SLM value could block unrelated settings changes. | Loading legacy settings normalizes this value to `off`, without opting into cloud processing. Actual legacy persistence/load/save regression was red, then green. |
| A fresh explicit recording could inherit retained samples from failed background capture. | Join/discard the failed producer before clearing/reconnecting; live handsfree capture remains reusable. Deterministic producer handoff regression was red, then green, and the live-reuse regression passes. |

The review also traced session ownership/lock ordering, model download/removal
exclusion, bounded HTTP reads, and native hotword capability handling. It found
no additional concrete blocker in those inspected paths.

After the final corrections, the reviewer inspected all six resolutions and
reported no remaining blocker in them. This was source and regression review;
the reviewer did not run live microphone or desktop input scenarios. The primary
agent independently ran the combined checks recorded in [validation.md](validation.md).
