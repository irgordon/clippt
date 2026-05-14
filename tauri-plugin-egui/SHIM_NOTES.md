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
  - `start_egui` currently accepts the callback and returns `Ok(())` while printing a compatibility-shim message.

No other public types, macros, extension methods, or features are implemented in this crate.

## What this shim intentionally does not implement

Based on the current code, this shim intentionally does **not** provide:

- Broad feature parity with any upstream `tauri-plugin-egui` crate.
- Any compatibility guarantee beyond this app's current compile-time API expectations.
- A migration layer for Tauri v2.
- A web frontend replacement.
- Additional runtime permissions.
- Persistence or clipboard behavior.
- A real egui render loop integration (the current `start_egui` implementation is a no-op bridge).

## When this shim should be removed

Remove this shim only after one of the following paths is completed and validated:

1. A compatible upstream egui/Tauri v1 integration is selected and integrated.
2. The app migrates away from egui to a standard Tauri web frontend.
3. The app migrates to Tauri v2 with a supported egui integration path.

After removing the local shim, validate all of the following successfully:

- `cargo check`
- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test --all-targets`
- `cargo tauri build`
