use crate::action::AppAction;
use crate::state::{ClipboardItem, Sensitivity, StoredItem};
use crate::AppState;
use egui::{Color32, RichText, TextureHandle, TextureOptions, Ui};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub struct ClipptUi {
    pub app_state: Arc<AppState>,
    pub action_tx: std::sync::mpsc::Sender<AppAction>,
    image_textures: HashMap<u64, TextureHandle>,
    search_query: String,
    show_settings: bool,
}

impl ClipptUi {
    pub fn new(app_state: Arc<AppState>, action_tx: std::sync::mpsc::Sender<AppAction>) -> Self {
        Self {
            app_state,
            action_tx,
            image_textures: HashMap::new(),
            search_query: String::new(),
            show_settings: false,
        }
    }

    pub fn update(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            self.render_header(ui);
            ui.separator();

            if self.show_settings {
                self.render_settings(ui);
            } else {
                self.render_status_line(ui);
                self.render_error_banner(ui);
                self.render_history(ui);
            }
        });
    }

    fn render_header(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.heading("Clippt");

            if ui.button("Settings").clicked() {
                self.show_settings = !self.show_settings;
            }

            if !self.show_settings {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Clear Search").clicked() {
                        self.search_query.clear();
                    }

                    ui.text_edit_singleline(&mut self.search_query);
                    ui.label("Search");
                });
            }
        });
    }

    fn render_settings(&mut self, ui: &mut Ui) {
        let mut current_settings = self.app_state.settings.lock().unwrap().clone();
        let mut changed = false;

        ui.heading("Privacy and capture settings");

        changed |= ui
            .checkbox(
                &mut current_settings.capture_enabled,
                "Enable clipboard capture",
            )
            .changed();

        changed |= ui
            .checkbox(
                &mut current_settings.persist_history,
                "Persist history to disk",
            )
            .changed();

        ui.add_enabled_ui(current_settings.persist_history, |ui| {
            ui.label("Persisted item types");

            changed |= ui
                .checkbox(&mut current_settings.persist_text, "Text")
                .changed();

            changed |= ui
                .checkbox(&mut current_settings.persist_images, "Images")
                .changed();

            changed |= ui
                .checkbox(&mut current_settings.persist_file_paths, "File paths")
                .changed();

            changed |= ui
                .checkbox(
                    &mut current_settings.clear_on_exit,
                    "Delete stored history on exit",
                )
                .changed();
        });

        changed |= ui
            .checkbox(
                &mut current_settings.filter_sensitive,
                "Prevent sensitive text from being persisted",
            )
            .changed();

        ui.separator();

        ui.label("History limits");

        changed |= ui
            .add(
                egui::DragValue::new(&mut current_settings.max_items)
                    .clamp_range(1..=10_000)
                    .prefix("Items: "),
            )
            .changed();

        changed |= ui
            .add(
                egui::DragValue::new(&mut current_settings.max_bytes)
                    .clamp_range(1..=1024 * 1024 * 1024)
                    .prefix("Bytes: "),
            )
            .changed();

        if changed {
            let _ = self
                .action_tx
                .send(AppAction::UpdateSettings(current_settings));
        }

        ui.separator();
        ui.heading("Data management");

        if ui.button("Clear in-memory history").clicked() {
            let _ = self.action_tx.send(AppAction::ClearInMemoryHistory);
        }

        if ui.button("Delete stored history now").clicked() {
            let _ = self.action_tx.send(AppAction::DeleteStoredHistory);
        }

        ui.label(
            RichText::new(
                "Deleting stored history does not disable future persistence if persistence remains enabled.",
            )
            .size(11.0)
            .color(Color32::GRAY),
        );

        ui.separator();

        if ui.button("Close settings").clicked() {
            self.show_settings = false;
        }
    }

    fn render_status_line(&self, ui: &mut Ui) {
        let settings = self.app_state.settings.lock().unwrap();

        let capture_status = if settings.capture_enabled {
            "Active"
        } else {
            "Paused"
        };

        let persistence_status = if settings.persist_history {
            "Persisting"
        } else {
            "Memory only"
        };

        let filter_status = if settings.filter_sensitive {
            "On"
        } else {
            "Off"
        };

        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("Capture: {}", capture_status))
                    .size(11.0)
                    .color(Color32::GRAY),
            );

            ui.label(RichText::new("|").size(11.0).color(Color32::DARK_GRAY));

            ui.label(
                RichText::new(format!("Persistence: {}", persistence_status))
                    .size(11.0)
                    .color(Color32::GRAY),
            );

            ui.label(RichText::new("|").size(11.0).color(Color32::DARK_GRAY));

            ui.label(
                RichText::new(format!("Sensitive filter: {}", filter_status))
                    .size(11.0)
                    .color(Color32::GRAY),
            );
        });

        ui.add_space(4.0);
    }

    fn render_error_banner(&mut self, ui: &mut Ui) {
        let error_message = self.app_state.latest_error.lock().unwrap().clone();

        if let Some(error_message) = error_message {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(error_message).color(Color32::RED));

                    if ui.button("Dismiss").clicked() {
                        let _ = self.action_tx.send(AppAction::DismissError);
                    }
                });
            });

            ui.add_space(4.0);
        }
    }

    fn render_history(&mut self, ui: &mut Ui) {
        let snapshot: Vec<StoredItem> = {
            let state = self.app_state.clipboard.lock().unwrap();
            state.items().cloned().collect()
        };

        if snapshot.is_empty() {
            ui.label(RichText::new("Clipboard history is empty").color(Color32::GRAY));
            return;
        }

        let mut active_texture_ids = HashSet::new();

        egui::ScrollArea::vertical().show(ui, |ui| {
            for stored in snapshot.iter().rev() {
                if !self.matches_search(&stored.item) {
                    continue;
                }

                ui.group(|ui| {
                    self.render_item(ui, stored, &mut active_texture_ids);
                });

                ui.add_space(4.0);
            }
        });

        self.image_textures
            .retain(|id, _| active_texture_ids.contains(id));
    }

    fn render_item(
        &mut self,
        ui: &mut Ui,
        stored: &StoredItem,
        active_texture_ids: &mut HashSet<u64>,
    ) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("#{}", stored.id))
                    .size(10.0)
                    .color(Color32::GRAY),
            );

            if stored.sensitivity == Sensitivity::Sensitive {
                ui.label(RichText::new("Sensitive").size(10.0).color(Color32::YELLOW));
            }
        });

        match &stored.item {
            ClipboardItem::Text(text) => {
                let display_text: Cow<'_, str> =
                    if let Some((idx, _)) = text.char_indices().nth(120) {
                        Cow::Owned(format!("{}...", &text[..idx]))
                    } else {
                        Cow::Borrowed(text.as_ref())
                    };

                let response = ui.button(display_text.as_ref());

                if response.clicked() {
                    let _ = self
                        .action_tx
                        .send(AppAction::CopyToClipboard(text.clone()));
                }

                if response.secondary_clicked() {
                    let _ = self.action_tx.send(AppAction::DeleteItem(stored.id));
                }
            }
            ClipboardItem::Image(width, height, bytes) => {
                let texture_id = stored.id;
                active_texture_ids.insert(texture_id);

                if let Some(texture) =
                    self.get_or_create_texture_checked(ui, texture_id, *width, *height, bytes)
                {
                    let size = texture.size_vec2();
                    let scale = if size.x > 250.0 { 250.0 / size.x } else { 1.0 };
                    let desired_size = size * scale;
                    let response = ui.add(egui::Image::new(texture.id(), desired_size));

                    if response.secondary_clicked() {
                        let _ = self.action_tx.send(AppAction::DeleteItem(stored.id));
                    }
                } else {
                    ui.label(RichText::new("Unsupported image format").color(Color32::GRAY));
                }
            }
            ClipboardItem::File(path) => {
                let response = ui.button(format!("File: {}", path.display()));

                if response.secondary_clicked() {
                    let _ = self.action_tx.send(AppAction::DeleteItem(stored.id));
                }
            }
        }
    }

    fn matches_search(&self, item: &ClipboardItem) -> bool {
        let query = self.search_query.trim();

        if query.is_empty() {
            return true;
        }

        match item {
            ClipboardItem::Text(text) => contains_case_insensitive_ascii(text, query),
            ClipboardItem::File(path) => {
                let path = path.to_string_lossy();
                contains_case_insensitive_ascii(&path, query)
            }
            ClipboardItem::Image(..) => {
                contains_case_insensitive_ascii("image img screenshot", query)
            }
        }
    }

    fn get_or_create_texture_checked(
        &mut self,
        ui: &Ui,
        texture_id: u64,
        width: usize,
        height: usize,
        data: &Arc<Vec<u8>>,
    ) -> Option<&TextureHandle> {
        let expected = width
            .checked_mul(height)
            .and_then(|pixels| pixels.checked_mul(4));

        if expected != Some(data.len()) {
            log::warn!(
                "Image bytes length mismatch for texture {}: expected {:?}, got {}.",
                texture_id,
                expected,
                data.len()
            );

            return None;
        }

        Some(self.get_or_create_texture(texture_id, width, height, data, ui))
    }

    fn get_or_create_texture(
        &mut self,
        texture_id: u64,
        width: usize,
        height: usize,
        data: &Arc<Vec<u8>>,
        ui: &Ui,
    ) -> &TextureHandle {
        self.image_textures.entry(texture_id).or_insert_with(|| {
            let image = egui::ColorImage::from_rgba_unmultiplied([width, height], data.as_slice());
            ui.ctx().load_texture(
                format!("clipimg_{}", texture_id),
                image,
                TextureOptions::default(),
            )
        })
    }
}

fn contains_case_insensitive_ascii(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }

    if !haystack.is_ascii() || !needle.is_ascii() {
        return haystack.contains(needle);
    }

    let haystack = haystack.as_bytes();
    let needle = needle.as_bytes();

    if needle.len() > haystack.len() {
        return false;
    }

    haystack.windows(needle.len()).any(|window| {
        window
            .iter()
            .zip(needle.iter())
            .all(|(left, right)| left.eq_ignore_ascii_case(right))
    })
}
