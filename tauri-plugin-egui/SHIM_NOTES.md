# tauri-plugin-egui shim notes

## Why this shim exists

This local crate exists to preserve Clippt's current Tauri v1 + egui architecture and keep the app compiling against the expected `tauri-plugin-egui` API boundary after upstream compatibility issues were encountered.

It is a compatibility bridge for this repository, not a claim of upstream parity.

## Public API surface provided by this shim

The shim currently exposes the following public items:

- `pub struct EguiPluginBuilder`
  - `impl Default for EguiPluginBuilder`
  - `pub fn build<R: tauri::Runtime>(self) -> tauri::plugin::TauriPlugin<R>`
- `pub trait EguiExt<R: tauri::Runtime>`
  - `fn start_egui<F>(&self, render: F) -> tauri::Result<()> where F: FnMut(&egui::Context) + Send + 'static`
- `impl<R: tauri::Runtime> EguiExt<R> for tauri::Window<R>`
  - `start_egui` now creates and owns a persistent `egui::Context`, starts a bounded background bridge loop, and repeatedly invokes the supplied callback while the window exists.

No other public types, macros, extension methods, or features are implemented in this crate.

## Current runtime behavior

- `start_egui(...)` **does invoke the callback** at runtime.
- The shim **does create and own an `egui::Context`** for each started window.
- The callback is driven by a small bridge loop that:
  - runs immediately,
  - re-runs on a bounded interval of about 16 ms,
  - and wakes earlier when `egui` requests repaint through `Context::set_request_repaint_callback`.
- The bridge loop stops when the Tauri window emits `CloseRequested` or `Destroyed`.
- The shim logs once when the bridge loop starts so runtime callback activation can be confirmed.

## What this shim still does not implement

Based on the current code, this shim intentionally does **not** provide:

- Broad feature parity with any upstream `tauri-plugin-egui` crate.
- Any compatibility guarantee beyond this app's current compile-time API expectations.
- A migration layer for Tauri v2.
- A web frontend replacement.
- Additional runtime permissions.
- Persistence or clipboard behavior.
- Real egui painting into the Tauri webview window.
- Input event translation from Tauri window events into egui input events.
- A GPU renderer, tessellation backend, or upstream-equivalent render integration.

## Release-readiness status

This shim is still only a **bridge**, not a release-ready egui/Tauri integration.

- Callback scheduling is implemented.
- Controller and UI callback execution can now be proven at runtime.
- **Visible egui rendering is still incomplete until a real painter/render backend is integrated.**

## When this shim should be removed

Remove this shim only after one of the following paths is completed and validated:

1. A compatible upstream egui/Tauri v1 integration is selected and integrated.
2. The app migrates away from egui to a standard Tauri web frontend.
3. The app migrates to Tauri v2 with a supported egui integration path.

Removal is appropriate only after the replacement both:

- invokes the Clippt render callback at runtime, and
- visibly renders the egui UI in the application window.

After removing the local shim, validate all of the following successfully:

- `cargo check`
- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test --all-targets`
- `cargo tauri build`
