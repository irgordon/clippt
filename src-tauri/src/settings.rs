use crate::persistence::atomic_write;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::AppHandle;

const SETTINGS_FILE_NAME: &str = "clippt_settings.json";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppSettings {
    pub capture_enabled: bool,
    pub persist_history: bool,
    pub persist_text: bool,
    pub persist_images: bool,
    pub persist_file_paths: bool,
    pub filter_sensitive: bool,
    pub clear_on_exit: bool,
    pub max_items: usize,
    pub max_bytes: usize,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            capture_enabled: true,
            persist_history: false,
            persist_text: true,
            persist_images: false,
            persist_file_paths: false,
            filter_sensitive: true,
            clear_on_exit: false,
            max_items: 100,
            max_bytes: 50 * 1024 * 1024,
        }
    }
}

pub fn load_settings(app_handle: &AppHandle) -> anyhow::Result<AppSettings> {
    let dir = app_config_dir(app_handle)?;
    load_settings_from_dir(&dir)
}

pub fn save_settings(app_handle: &AppHandle, settings: &AppSettings) -> anyhow::Result<()> {
    let dir = app_config_dir(app_handle)?;
    save_settings_to_dir(&dir, settings)
}

pub fn load_settings_from_dir(dir: &Path) -> anyhow::Result<AppSettings> {
    let path = settings_path(dir);

    if !path.exists() {
        return Ok(AppSettings::default());
    }

    let bytes = fs::read(path)?;
    let settings = serde_json::from_slice::<AppSettings>(&bytes)?;
    Ok(settings)
}

pub fn save_settings_to_dir(dir: &Path, settings: &AppSettings) -> anyhow::Result<()> {
    fs::create_dir_all(dir)?;
    let bytes = serde_json::to_vec_pretty(settings)?;
    atomic_write(&settings_path(dir), &bytes)
}

fn app_config_dir(app_handle: &AppHandle) -> anyhow::Result<PathBuf> {
    app_handle
        .path_resolver()
        .app_config_dir()
        .ok_or_else(|| anyhow::anyhow!("no config dir"))
}

fn settings_path(dir: &Path) -> PathBuf {
    dir.join(SETTINGS_FILE_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_privacy_preserving() {
        let settings = AppSettings::default();

        assert!(settings.capture_enabled);
        assert!(!settings.persist_history);
        assert!(settings.persist_text);
        assert!(!settings.persist_images);
        assert!(!settings.persist_file_paths);
        assert!(settings.filter_sensitive);
        assert!(!settings.clear_on_exit);
        assert_eq!(settings.max_items, 100);
        assert_eq!(settings.max_bytes, 50 * 1024 * 1024);
    }
}
