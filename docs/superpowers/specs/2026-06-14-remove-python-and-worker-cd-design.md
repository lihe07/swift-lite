# Remove Legacy Python Backend + rs-worker CD — Design

**Date:** 2026-06-14
**Status:** Approved

## Goal

Three independent changes that finish migrating the project off Python and make
the Rust ML worker distributable:

1. Delete the legacy Python backend (`back/`) and the legacy Python detection
   workers + model files (`det/`).
2. Make `rs-worker` prompt for its worker name with a native Windows modal
   (built with `native-windows-gui`) when no name is supplied on the command
   line.
3. Add a GitHub Actions CD workflow that builds `rs-worker` for Windows and
   Linux on version tags and attaches the binaries to a GitHub Release.

These three are orthogonal and can be implemented and verified independently.

## Context

- `rs-back/` is a drop-in Rust reimplementation of `back/` (axum + tokio +
  tokio-postgres). `rs-worker/` is the Rust ML worker that replaced the Python
  detection scripts in `det/`.
- No code references `back/` or `det/` — only the design/plan docs under
  `docs/superpowers/` mention them. Both directories are fully superseded.
- `back/__pycache__/*.pyc`, `det/best.onnx`, and `det/best.torchscript.pt` are
  tracked in git, so removal must use `git rm -r` (not just `rm`).
- `rs-worker` embeds its ONNX model via `include_bytes!("../best.onnx")`
  (`rs-worker/src/detect.rs`), and links ONNX Runtime through the `ort` crate's
  default `download-binaries` feature (CPU execution provider, statically linked
  at build time). The release binary is therefore self-contained — no model file
  or ORT shared library needs to be shipped alongside it. The checked-in
  `rs-worker/ort.so` is an unused leftover from an earlier dynamic-loading
  experiment and is not consumed by the default build.

## 1. Remove legacy Python (`back/` + `det/`)

`git rm -r back det`.

- Nothing in the codebase imports or shells out to either directory.
- The existing design/plan docs that reference `back/`/`det/` are historical
  records and are left unchanged (they describe the rewrite that already
  happened).
- Net effect: ~80 MB of tracked binaries (`.onnx`, `.torchscript.pt`, `.pyc`)
  leave the working tree and future clones.

No code changes are required elsewhere.

## 2. Windows name modal in `rs-worker`

### Current behavior

`rs-worker/src/main.rs` reads the worker name from `argv[1]`, defaulting to
`"worker"`:

```rust
let name = args.get(1).cloned().unwrap_or("worker".to_string());
```

The name is used in the `ping` response so the master can identify the worker.

### New behavior

Name resolution becomes:

1. If `argv[1]` is present → use it (unchanged; works identically on all
   platforms and keeps the worker scriptable).
2. Else, on Windows → pop a native modal asking for the name.
3. Else (non-Windows, no arg) → `"worker"` (unchanged Linux/macOS behavior).

If the modal is closed, cancelled, or submitted empty, fall back to `"worker"`.

### Implementation

- **Dependencies** — add a Windows-only target block to `rs-worker/Cargo.toml`
  so the Linux build never compiles Win32 crates:

  ```toml
  [target.'cfg(windows)'.dependencies]
  native-windows-gui = "1"
  native-windows-derive = "1"
  ```

- **New module `rs-worker/src/prompt.rs`**, entirely gated behind
  `#[cfg(windows)]`. It exposes one function:

  ```rust
  /// Show a modal asking for the worker name. Returns the trimmed input,
  /// or None if the dialog was cancelled/closed or left empty.
  pub fn ask_worker_name() -> Option<String>;
  ```

  The window is a small fixed-size dialog containing a label, a single-line
  `TextInput`, and an OK button. Pressing Enter or clicking OK confirms and
  closes the dialog; closing the window cancels. The function runs a synchronous
  native message loop and returns once the dialog closes.

- **`main.rs`** wires it in before the async reconnect loop:

  ```rust
  let name = match args.get(1).cloned() {
      Some(n) => n,
      None => resolve_name_interactive(),
  };
  ```

  where `resolve_name_interactive()` is `#[cfg(windows)]` →
  `prompt::ask_worker_name().unwrap_or_else(|| "worker".into())` and
  `#[cfg(not(windows))]` → `"worker".into()`.

- The modal is shown **once at startup, on the main thread, before** entering the
  `#[tokio::main]` reconnect loop. It is a blocking native call; the tokio
  runtime is already started by the `#[tokio::main]` macro, but no async work has
  begun yet, so blocking the main thread here is fine.

### Console vs GUI subsystem

The binary keeps the **default console subsystem** (no
`#![windows_subsystem = "windows"]`). The worker is a long-running process whose
`println!` status output (connection state, detection timings) is meant to be
watched, so the console stays. The modal appears as a separate dialog window at
startup.

## 3. GitHub CD workflow

New file `.github/workflows/release.yml`.

- **Trigger:** push of a tag matching `v*`.
- **Build matrix:**
  | Runner | Target | Output name |
  |---|---|---|
  | `ubuntu-latest` | `x86_64-unknown-linux-gnu` | `rs-worker-linux-x86_64` |
  | `windows-latest` | `x86_64-pc-windows-msvc` | `rs-worker-windows-x86_64.exe` |
- **Build steps per matrix entry:**
  1. `actions/checkout`
  2. `dtolnay/rust-toolchain@stable`
  3. `Swatinem/rust-cache` scoped to `rs-worker/`
  4. `cargo build --release --manifest-path rs-worker/Cargo.toml`
  5. Rename `target/release/rs-worker[.exe]` to the platform output name
  6. `actions/upload-artifact` for the renamed binary
- **Release job** (depends on both builds): download both artifacts and attach
  them to the tag's GitHub Release via `softprops/action-gh-release`.
- The `ort` crate downloads prebuilt ONNX Runtime binaries during the build;
  GitHub runners have network access, so this works on both runners.
- Binaries are self-contained (embedded model + statically-linked ORT). Nothing
  else is shipped.

## Testing / verification

1. **Removal:** `git rm -r back det`; confirm `cargo build` for `rs-back` and
   `rs-worker` still succeed and `git grep -l 'back/\|det/'` only matches docs.
2. **Worker modal:** `cargo build` on Linux must still succeed without pulling in
   `native-windows-gui` (verify it is absent from the Linux build's resolved
   features / does not appear in a `cargo build` on Linux). Windows behavior is
   verified through the CD build producing a working `.exe`; interactive modal
   testing is manual on a Windows machine.
3. **CD:** push a `v*` tag (or run the matrix build on a branch via a temporary
   `workflow_dispatch` during development) and confirm both binaries build and
   upload, and that the Linux binary runs and connects with a CLI-supplied name.

## Out of scope

- No changes to `rs-back`, the frontend, the database, or the TCP protocol.
- No macOS build target.
- No code signing / notarization of the Windows binary.
- No `workflow_dispatch` trigger in the final workflow (tag-only), though one may
  be added temporarily during development.
