# Repository bug audit and repairs

Scope: the OpenDictate repository, reviewed from `cda12cb`. The three review roles
ran sequentially before production changes. Local branch:
`fix/repo-bug-audit-2026-10-19`.

- **64 candidates reported; 63 confirmed; B063 dismissed.**
- **Confirmed severity:** 6 Critical, 47 Medium, 10 Low.
- **Finder score:** 305. **Skeptic:** one supported dismissal worth 5 provisional
  points. Original accepted Finder scores total 300; severity-adjusted confirmed
  points total 305 after B019's upgrade. No external ground-truth arbiter score
  is claimed.

Full role outputs retain each requested claim, counterargument, confidence,
verdict, and scoring/risk calculation:

1. [Bug Finder](finder.md)
2. [Bug Skeptic](skeptic.md)
3. [Final Arbiter](arbiter.md)

The following map links every confirmed finding to its repair and regression
evidence. [Validation](validation.md) records aggregate checks and unexecuted
platform scenarios. [Independent patch review](review.md) records the six
implementation corrections and their final disposition. Source repairs and automated coverage are distinguished from
hardware verification in the reports; this audit cannot prove the absence of all
other bugs.

| ID | Severity | Confirmed finding | Implementation and evidence |
|---|---|---|---|
| B001 | Critical | Microphone test side effects | [Sessions](sessions-fixes.md), [Interface](frontend-fixes.md) |
| B002 | Medium | Streaming final audio loss | [Sessions](sessions-fixes.md) |
| B003 | Medium | Inconsistent final transcript processing | [Sessions](sessions-fixes.md) |
| B004 | Medium | Unreachable streaming caption fallback | [Sessions](sessions-fixes.md) |
| B005 | Medium | Stale caption workers | [Sessions](sessions-fixes.md) |
| B006 | Medium | Unserialized session operations and stale commits | [Sessions](sessions-fixes.md), [Interface](frontend-fixes.md) |
| B007 | Medium | Warmup mutates active recording state | [Sessions](sessions-fixes.md) |
| B008 | Medium | Handsfree and explicit sessions process the same audio | [Sessions](sessions-fixes.md) |
| B009 | Medium | Failed handsfree enable wedges state | [Sessions](sessions-fixes.md) |
| B010 | Medium | Running handsfree ignores changed configuration | [Sessions](sessions-fixes.md) |
| B011 | Medium | Handsfree duration limit is conditional on silence | [Sessions](sessions-fixes.md) |
| B012 | Medium | Continuous offline capture has processing gaps | [Sessions](sessions-fixes.md) |
| B013 | Low | Continuous meter worker can exit permanently | [Sessions](sessions-fixes.md) |
| B014 | Medium | Clearing capture invalidates watermarks | [Core](core-fixes.md) |
| B015 | Medium | Long manual offline recording loses its beginning | [Sessions](sessions-fixes.md) |
| B016 | Medium | CPAL capture unconditionally requests f32 | [Core](core-fixes.md) |
| B017 | Critical | PulseAudio stop/setup can block indefinitely | [Core](core-fixes.md) |
| B018 | Medium | Capture failure leaves logical recording active | [Core](core-fixes.md) |
| B019 | Critical | Unsupported dictionary hotword configuration can terminate the app | [Core](core-fixes.md) |
| B020 | Medium | Dictionary punctuation/casing corruption | [Core](core-fixes.md) |
| B021 | Medium | Annotation filter removes normal content | [Core](core-fixes.md) |
| B022 | Medium | Fuzzy match scores exceed their range | [Core](core-fixes.md) |
| B023 | Medium | Command matching rewrites payload data | [Core](core-fixes.md), [Native processing](sessions-fixes.md) |
| B024 | Medium | Truncated polish replaces complete text | [Core](core-fixes.md) |
| B025 | Medium | Unimplemented Local SLM is selectable | [Interface](frontend-fixes.md), [Saved-setting migration](native-fixes.md) |
| B026 | Critical | Recognized text crosses a script boundary | [Native/data](native-fixes.md) |
| B027 | Critical | Failed insertion can still submit existing content | [Sessions](sessions-fixes.md) |
| B028 | Medium | Clipboard failure returns success | [Native/data](native-fixes.md) |
| B029 | Medium | Fixed key bindings violate advertised target actions | [Interface](frontend-fixes.md), [Native/data](native-fixes.md) |
| B030 | Medium | Malformed wtype combinations | [Native/data](native-fixes.md) |
| B031 | Medium | Main-window Insert/Undo targets its own focus | [Interface](frontend-fixes.md), [Native/data](native-fixes.md) |
| B032 | Critical | Arbitrary model removal paths | [Core](core-fixes.md) |
| B033 | Medium | Cancel/retry loses download ownership | [Sessions](sessions-fixes.md) |
| B034 | Medium | Background installs bypass deduplication | [Core](core-fixes.md) |
| B035 | Medium | Download read cannot promptly observe cancellation | [Core](core-fixes.md) |
| B036 | Medium | KWS readiness accepts empty files | [Core](core-fixes.md) |
| B037 | Medium | Installation completion is confused with download launch/bytes | [Interface](frontend-fixes.md) |
| B038 | Medium | Download All reads stale pending state | [Interface](frontend-fixes.md) |
| B039 | Medium | Onboarding does not ensure the selected STT is usable | [Interface](frontend-fixes.md) |
| B040 | Medium | One CPU benchmark disables all streaming choices | [Interface](frontend-fixes.md) |
| B041 | Medium | CSV column order is wrong | [Native/data](native-fixes.md) |
| B042 | Medium | History export drops older rows | [Native/data](native-fixes.md) |
| B043 | Medium | Pre-reset history mutates post-reset aggregates | [Native/data](native-fixes.md) |
| B044 | Low | Statistics count spaces instead of words | [Native/data](native-fixes.md) |
| B045 | Low | Zero-word today row breaks yesterday's streak | [Native/data](native-fixes.md) |
| B046 | Medium | Reset Settings omits runtime/OS transitions | [Sessions](sessions-fixes.md) |
| B047 | Medium | Hydration confuses hardware capture with a user recording | [Sessions](sessions-fixes.md) |
| B048 | Medium | Dock lacks its event capability | [Native/data](native-fixes.md) |
| B049 | Low | Dock flash cleanup cancels its only expiry | [Interface](frontend-fixes.md) |
| B050 | Medium | Undo status survives a new result | [Interface](frontend-fixes.md) |
| B051 | Low | Select-all checks cardinality, not visible membership | [Interface](frontend-fixes.md) |
| B052 | Medium | Snippet Test Insert does not test expansion | [Interface](frontend-fixes.md), [Native/data](native-fixes.md) |
| B053 | Low | Mic-test bars have zero height | [Interface](frontend-fixes.md) |
| B054 | Low | Resolved event listeners leak on unmount | [Interface](frontend-fixes.md) |
| B055 | Medium | Installed GNOME hotkey requires development files | [Native/data](native-fixes.md) |
| B056 | Low | Typed empty GNOME array is parsed as a path | [Native/data](native-fixes.md) |
| B057 | Medium | Failed shortcut replacement disarms old macOS gesture | [Native/data](native-fixes.md) |
| B058 | Medium | Hold-to-talk worker ordering is not event ordering | [Native/data](native-fixes.md) |
| B059 | Medium | AppImage autostart persists a temporary mount path | [Native/data](native-fixes.md) |
| B060 | Low | Autostart plist embeds unescaped XML text | [Native/data](native-fixes.md) |
| B061 | Medium | Experimental GPU workflow is internally inconsistent | [Native/data](native-fixes.md) |
| B062 | Low | Model tests share mutable global directory state | [Core](core-fixes.md) |
| B064 | Medium | Handsfree onset confirmation drops consumed audio | [Sessions](sessions-fixes.md) |

## Behavior changes to review

- Microphone tests only measure audio; they do not transcribe, run actions, save
  history, or call a polish provider.
- Dictation cancellation prevents stale results from committing. Long manual
  recordings finalize before the capture buffer fills; an overloaded continuous
  queue stops visibly and drains captured audio.
- Model installation remains busy through extraction/dependencies. Cancellation
  retains ownership until cleanup completes.
- Insert/Undo return focus to the previous application, reject own-window focus,
  and retain retry state after failure. Undo requires a confirmed latest insertion.
- Settings include **Text fields** and **Shell (Bash, Zsh)** input targets for
  their different editing shortcuts.
- Unavailable Local SLM cannot be selected. Legacy saved values normalize to
  unpolished local processing; no cloud service is enabled by migration.
- History exports include every row. Statistics resets isolate old history from
  new totals. Legacy resets without a recorded boundary retain their totals and
  conservatively exclude existing rows from future adjustments.

No remote push, deployment, live production database mutation, or destructive
input testing is part of this work.
