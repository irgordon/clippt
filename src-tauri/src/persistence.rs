use crate::settings::AppSettings;
use crate::state::{ClipboardItem, Sensitivity, StoredItem};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use tauri::AppHandle;

#[derive(Serialize, Deserialize)]
struct PersistedState {
    schema_version: u32,
    items: Vec<PersistedItem>,
}

#[derive(Serialize, Deserialize)]
struct PersistedItem {
    id: u64,
    kind: String,
    text: Option<String>,
    file: Option<String>,
    image_file: Option<String>,
    width: Option<usize>,
    height: Option<usize>,
}

pub fn should_persist_item(item: &StoredItem, settings: &AppSettings) -> bool {
    if !settings.persist_history {
        return false;
    }

    if settings.filter_sensitive && item.sensitivity == Sensitivity::Sensitive {
        return false;
    }

    match &item.item {
        ClipboardItem::Text(_) => settings.persist_text,
        ClipboardItem::Image(_, _, _) => settings.persist_images,
        ClipboardItem::File(_) => settings.persist_file_paths,
    }
}

pub fn save_state(
    app_handle: &AppHandle,
    items: &[StoredItem],
    settings: &AppSettings,
) -> anyhow::Result<()> {
    let cache = app_cache_dir(app_handle)?;
    save_state_to_dir(&cache, items, settings)
}

pub fn load_state(app_handle: &AppHandle) -> anyhow::Result<Vec<StoredItem>> {
    let cache = app_cache_dir(app_handle)?;
    load_state_from_dir(&cache)
}

pub fn save_state_to_dir(
    cache: &Path,
    items: &[StoredItem],
    settings: &AppSettings,
) -> anyhow::Result<()> {
    fs::create_dir_all(cache)?;

    let mut persisted = Vec::new();
    let mut active_image_files = HashSet::new();

    for item in items
        .iter()
        .filter(|item| should_persist_item(item, settings))
    {
        match &item.item {
            ClipboardItem::Text(text) => {
                persisted.push(PersistedItem {
                    id: item.id,
                    kind: "text".to_string(),
                    text: Some(text.to_string()),
                    file: None,
                    image_file: None,
                    width: None,
                    height: None,
                });
            }
            ClipboardItem::File(path) => {
                persisted.push(PersistedItem {
                    id: item.id,
                    kind: "file".to_string(),
                    text: None,
                    file: Some(path.to_string_lossy().into_owned()),
                    image_file: None,
                    width: None,
                    height: None,
                });
            }
            ClipboardItem::Image(width, height, bytes) => {
                let file_name = format!("clipimg_{}.bin", item.id);
                let image_path = cache.join(&file_name);

                let should_write = match fs::metadata(&image_path) {
                    Ok(metadata) => metadata.len() != bytes.len() as u64,
                    Err(_) => true,
                };

                if should_write {
                    atomic_write(&image_path, bytes.as_slice())?;
                }

                active_image_files.insert(file_name.clone());

                persisted.push(PersistedItem {
                    id: item.id,
                    kind: "image".to_string(),
                    text: None,
                    file: None,
                    image_file: Some(file_name),
                    width: Some(*width),
                    height: Some(*height),
                });
            }
        }
    }

    let state = PersistedState {
        schema_version: 1,
        items: persisted,
    };

    let json = serde_json::to_vec(&state)?;
    atomic_write(&cache.join("clippt_history.json"), &json)?;

    cleanup_orphaned_images(cache, &active_image_files)?;

    Ok(())
}

pub fn load_state_from_dir(cache: &Path) -> anyhow::Result<Vec<StoredItem>> {
    remove_stale_temp_files(cache)?;

    let path = cache.join("clippt_history.json");

    if !path.exists() {
        return Ok(Vec::new());
    }

    let data = fs::read(&path)?;

    let state: PersistedState = match serde_json::from_slice(&data) {
        Ok(state) => state,
        Err(error) => {
            let corrupt_path = cache.join("clippt_history.corrupt.json");
            let _ = fs::remove_file(&corrupt_path);
            let _ = fs::rename(&path, &corrupt_path);

            return Err(anyhow::anyhow!("persistent state is corrupt: {}", error));
        }
    };

    if state.schema_version != 1 {
        return Err(anyhow::anyhow!(
            "unsupported persistence schema version: {}",
            state.schema_version
        ));
    }

    let mut restored = Vec::new();

    for item in state.items {
        match item.kind.as_str() {
            "text" => {
                let text = item.text.unwrap_or_default();

                restored.push(StoredItem {
                    id: item.id,
                    item: ClipboardItem::Text(std::sync::Arc::<str>::from(text)),
                    sensitivity: Sensitivity::Normal,
                });
            }
            "file" => {
                restored.push(StoredItem {
                    id: item.id,
                    item: ClipboardItem::File(PathBuf::from(item.file.unwrap_or_default())),
                    sensitivity: Sensitivity::Normal,
                });
            }
            "image" => {
                let Some(file_name) = item.image_file else {
                    log::warn!("Skipping persisted image id={} with no image file.", item.id);
                    continue;
                };

                let image_path = cache.join(file_name);

                let bytes = match fs::read(&image_path) {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        log::warn!(
                            "Skipping persisted image id={} because blob could not be read: {}",
                            item.id,
                            error
                        );
                        continue;
                    }
                };

                restored.push(StoredItem {
                    id: item.id,
                    item: ClipboardItem::Image(
                        item.width.unwrap_or(0),
                        item.height.unwrap_or(0),
                        std::sync::Arc::new(bytes),
                    ),
                    sensitivity: Sensitivity::Normal,
                });
            }
            other => {
                log::warn!("Skipping unknown persisted clipboard item kind: {}", other);
            }
        }
    }

    Ok(restored)
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow::anyhow!("invalid persistence path"))?;

    let tmp_path = path.with_file_name(format!(".clippt_{}.{}.tmp", file_name, std::process::id()));

    {
        let mut file = File::create(&tmp_path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }

    fs::rename(&tmp_path, path)?;

    Ok(())
}

pub fn delete_persisted_history(app_handle: &AppHandle) -> anyhow::Result<()> {
    let cache = app_cache_dir(app_handle)?;
    delete_persisted_history_from_dir(&cache)
}

pub fn delete_persisted_history_from_dir(cache: &Path) -> anyhow::Result<()> {
    for file_name in [
        "clippt_history.json",
        "clippt_history.json.tmp",
        "clippt_history.tmp",
        "clippt_history.corrupt.json",
    ] {
        remove_file_if_present(&cache.join(file_name));
    }

    remove_stale_temp_files(cache)?;
    cleanup_orphaned_images(cache, &HashSet::new())?;

    log::info!("Explicitly deleted stored clipboard history.");

    Ok(())
}

fn app_cache_dir(app_handle: &AppHandle) -> anyhow::Result<PathBuf> {
    app_handle
        .path_resolver()
        .app_cache_dir()
        .ok_or_else(|| anyhow::anyhow!("no cache dir"))
}

fn remove_file_if_present(path: &Path) {
    if let Err(error) = fs::remove_file(path) {
        if error.kind() != std::io::ErrorKind::NotFound {
            log::warn!("Failed to remove {}: {}", path.display(), error);
        }
    }
}

fn remove_stale_temp_files(cache_dir: &Path) -> anyhow::Result<()> {
    if !cache_dir.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(cache_dir)? {
        let entry = entry?;
        let path = entry.path();

        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };

        if file_name.starts_with(".clippt_") && file_name.ends_with(".tmp") {
            remove_file_if_present(&path);
        }
    }

    Ok(())
}

fn cleanup_orphaned_images(cache_dir: &Path, active_images: &HashSet<String>) -> anyhow::Result<()> {
    if !cache_dir.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(cache_dir)? {
        let entry = entry?;
        let path = entry.path();

        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };

        if file_name.starts_with("clipimg_")
            && file_name.ends_with(".bin")
            && !active_images.contains(file_name)
        {
            log::debug!("Garbage collecting orphaned image file: {}", file_name);
            remove_file_if_present(&path);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{ClipboardItem, Sensitivity, StoredItem};
    use std::sync::Arc;

    fn base_settings() -> AppSettings {
        AppSettings {
            persist_history: true,
            persist_text: true,
            persist_images: false,
            persist_file_paths: false,
            filter_sensitive: true,
            ..AppSettings::default()
        }
    }

    #[test]
    fn should_skip_all_items_when_persistence_disabled() {
        let settings = AppSettings {
            persist_history: false,
            ..base_settings()
        };

        let item = StoredItem {
            id: 1,
            item: ClipboardItem::Text(Arc::<str>::from("hello")),
            sensitivity: Sensitivity::Normal,
        };

        assert!(!should_persist_item(&item, &settings));
    }

    #[test]
    fn should_persist_text_when_enabled() {
        let settings = base_settings();

        let item = StoredItem {
            id: 1,
            item: ClipboardItem::Text(Arc::<str>::from("hello")),
            sensitivity: Sensitivity::Normal,
        };

        assert!(should_persist_item(&item, &settings));
    }

    #[test]
    fn should_skip_sensitive_text_when_filter_enabled() {
        let settings = base_settings();

        let item = StoredItem {
            id: 1,
            item: ClipboardItem::Text(Arc::<str>::from("secret")),
            sensitivity: Sensitivity::Sensitive,
        };

        assert!(!should_persist_item(&item, &settings));
    }

    #[test]
    fn should_persist_sensitive_text_when_filter_disabled() {
        let settings = AppSettings {
            filter_sensitive: false,
            ..base_settings()
        };

        let item = StoredItem {
            id: 1,
            item: ClipboardItem::Text(Arc::<str>::from("secret")),
            sensitivity: Sensitivity::Sensitive,
        };

        assert!(should_persist_item(&item, &settings));
    }

    #[test]
    fn save_and_load_roundtrip_from_dir() {
        let dir = tempfile::tempdir().unwrap();
        let settings = base_settings();

        let items = vec![StoredItem {
            id: 42,
            item: ClipboardItem::Text(Arc::<str>::from("hello world")),
            sensitivity: Sensitivity::Normal,
        }];

        save_state_to_dir(dir.path(), &items, &settings).unwrap();
        let loaded = load_state_from_dir(dir.path()).unwrap();

        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, 42);

        match &loaded[0].item {
            ClipboardItem::Text(text) => assert_eq!(text.as_ref(), "hello world"),
            _ => panic!("expected text item"),
        }
    }

    #[test]
    fn delete_persisted_history_from_dir_removes_history_and_images() {
        let dir = tempfile::tempdir().unwrap();

        fs::write(dir.path().join("clippt_history.json"), b"{}").unwrap();
        fs::write(dir.path().join("clipimg_1.bin"), b"image").unwrap();
        fs::write(dir.path().join(".clippt_clippt_history.json.123.tmp"), b"tmp").unwrap();

        delete_persisted_history_from_dir(dir.path()).unwrap();

        assert!(!dir.path().join("clippt_history.json").exists());
        assert!(!dir.path().join("clipimg_1.bin").exists());
        assert!(!dir.path().join(".clippt_clippt_history.json.123.tmp").exists());
    }
}
