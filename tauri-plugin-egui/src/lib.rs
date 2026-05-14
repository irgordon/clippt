use tauri::{plugin::TauriPlugin, Runtime};

/// Minimal compatibility shim that preserves the existing Clippt architecture
/// boundary (`tauri-plugin-egui` + `window.start_egui(...)`) for compile
/// and test workflows in this repository.
pub struct EguiPluginBuilder;

impl Default for EguiPluginBuilder {
    fn default() -> Self {
        Self
    }
}

impl EguiPluginBuilder {
    pub fn build<R: Runtime>(self) -> TauriPlugin<R> {
        tauri::plugin::Builder::new("egui").build()
    }
}

pub trait EguiExt<R: Runtime> {
    fn start_egui<F>(&self, render: F) -> tauri::Result<()>
    where
        F: FnMut(&egui::Context) + Send + 'static;
}

impl<R: Runtime> EguiExt<R> for tauri::Window<R> {
    fn start_egui<F>(&self, render: F) -> tauri::Result<()>
    where
        F: FnMut(&egui::Context) + Send + 'static,
    {
        let _ = &render;
        eprintln!(
            "tauri-plugin-egui compatibility shim active: start_egui is currently a no-op."
        );
        Ok(())
    }
}
