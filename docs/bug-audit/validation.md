# Bug audit validation

Audit base: `cda12cb` on `main`. Scope: the OpenDictate repository. The finder,
skeptic, and arbiter reviews ran sequentially before production changes.
Existing untracked `.codex-logs/` is outside this work.

## Final verification

Run on 2026-10-19, after the integrated repairs and review corrections:

| Check | Result |
|---|---|
| `cargo test --workspace --lib --locked` | **162 passed, 0 failed**: 55 app tests and 107 core tests. Five core integration tests are ignored in the default run; two were explicitly run below. |
| `cargo test -p opendictate-core --lib --locked installed_ -- --ignored --nocapture` | **2 passed, 0 failed** using existing local model assets. Offline NeMo hotwords and streaming hotwords/finalization exercised native sherpa-onnx. No model download. |
| `npm test -- --reporter=dot` | **35 passed, 0 failed**. Component interactions and shared state are real; Tauri IPC/event boundaries are mocked. |
| `npm run build` | **Passed**: TypeScript and Vite production build. |
| `python3 -m unittest discover -s scripts -p 'test_*.py'` | **3 passed**: archive formats, path confinement, and preservation of packaging configuration. |
| `cargo clippy --workspace --all-targets --locked` | **Passed with seven existing style warnings**, six in core and one in `dock.rs`. It is not warning-free. |
| `rustfmt --edition 2021 --config skip_children=true --check` on every changed/new Rust source | **Passed**, 23 files. Unrelated existing formatting drift was left outside this change. |
| `git diff --check` | **Passed**. |

Additional focused verification:

- Headless Chromium rendered the real microphone meter with simulated IPC:
  all 24 bars measured 16 px high; screenshot inspected.
- Launcher scripts passed shell syntax and a portable installed/AppImage path
  fixture containing spaces. The GPU workflow passed YAML and shell syntax
  inspection; its Tauri CLI arguments were checked against the installed CLI.
- `cargo tree -p opendictate --locked --no-default-features --features gpu-shared
  -e features -i sherpa-onnx` confirmed shared linking without the static feature.
- [Patch review](review.md) identified six corrections across two passes; all
  were repaired, and a final targeted review found no remaining blocker in them.
- Per-finding reproduction and regression evidence is linked from the complete
  [63-finding repair map](fixes.md). Tests requiring native side effects are
  distinguished from deterministic regressions in those reports.

## Baseline

- `npm run build`: passed. Vite reports the existing bundle-size warning.
- `cargo test --workspace --locked`: compilation stopped with `No space left on
  device` while building `libopendictate_lib.a`; no test verdict from this run.
  Removed only regenerable outputs with `cargo clean -p opendictate` (3.6 GiB)
  and switched to workspace library tests to avoid unnecessary static archives.
- `cargo test --workspace --lib --locked`: passed, 98 tests; 3 hardware/model
  integration tests are explicitly ignored by the existing suite.
- `cargo fmt --all --check`: failed on extensive pre-existing formatting drift,
  including ignored local examples. This is not classified as a product bug.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: failed on
  six existing style lints in the core crate (`chunks_exact_to_as_chunks`,
  needless borrows, collapsible conditions, `map_or`, and character matching).
  The raw baseline is saved locally in `/tmp/opendictate-audit-clippy-baseline.log`.
- `cargo clippy --workspace --all-targets --locked`: passed with existing style
  warnings in the core and native shell; no compilation errors.
- Frontend had a production type/build check but no behavior test command at
  the audit base. This change adds the behavior suite reported above.

## Limits

Review and tests establish specific bugs and regressions; they cannot prove the
absence of all other bugs. The following checks were **not executed**:

- Live microphone capture, real PulseAudio server interruption, sustained
  handsfree/continuous load, and the full 110-second manual recording limit.
  Two hardware-dependent PulseAudio tests and the existing WAV recognition test
  remain ignored. Local model smoke tests used synthetic audio and do not
  establish recognition accuracy.
- Real input into external applications, compositor clipboard ownership,
  window-focus restoration, installed global shortcuts, and autostart changes.
- Native Windows/macOS builds and OS behavior. Linux compilation does not
  validate their platform-gated branches.
- Full desktop installer creation, GPU workflow dispatch/installer execution,
  and CUDA hardware acceleration. The initial static-library build hit the
  local disk limit; successful library tests and Clippy do not substitute for
  producing an installer.
- Live cloud polish calls or changes to production/user databases. Database
  regressions use temporary fixtures. Legacy statistics resets without a saved
  boundary preserve existing totals and exclude existing rows from subsequent
  adjustments, as explained in [native repairs](native-fixes.md).

No remote push or deployment was performed. These limits remain release smoke
checks, rather than evidence that the corresponding code paths are bug-free.
