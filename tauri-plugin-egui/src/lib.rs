use tauri::{plugin::TauriPlugin, Runtime};

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
    fn start_egui<F>(&self, _render: F) -> tauri::Result<()>
    where
        F: FnMut(&egui::Context) + Send + 'static,
    {
        Ok(())
    }
}
