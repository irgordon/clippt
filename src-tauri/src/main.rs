#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod action;
mod listener;
mod persistence;
mod persistence_worker;
mod privacy;
mod settings;
mod state;
mod ui;

use crate::action::AppAction;
use crate::persistence::load_state;
use crate::persistence_worker::{
    spawn_persistence_worker, wait_for_worker_ack, PersistenceCommand, PersistenceEvent,
};
use crate::privacy::PrivacyGuard;
use crate::settings::{load_settings, AppSettings};
use crate::state::{ClipboardItem, ClipboardState, ClipptError, Sensitivity, StoredItem};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{CustomMenuItem, Manager, SystemTray, SystemTrayEvent, SystemTrayMenu};
use tauri_plugin_egui::EguiExt;

pub struct AppState {
    pub clipboard: Mutex<ClipboardState>,
    pub receiver: Mutex<mpsc::Receiver<Result<ClipboardItem, ClipptError>>>,
    pub action_rx: Mutex<mpsc::Receiver<AppAction>>,
    pub persistence_tx: mpsc::Sender<PersistenceCommand>,
    pub persistence_event_rx: Mutex<mpsc::Receiver<PersistenceEvent>>,
    pub latest_error: Mutex<Option<String>>,
    pub settings: Mutex<AppSettings>,
    pub is_capturing: Arc<AtomicBool>,
}

struct SaveDebouncer {
    last_save: Instant,
    debounce_duration: Duration,
}

impl SaveDebouncer {
    fn new() -> Self {
        Self {
            last_save: Instant::now(),
            debounce_duration: Duration::from_secs(3),
        }
    }

    fn should_save(&self) -> bool {
        self.last_save.elapsed() >= self.debounce_duration
    }

    fn mark_saved(&mut self) {
        self.last_save = Instant::now();
    }
}

fn classify_item_for_memory(item: ClipboardItem) -> (ClipboardItem, Sensitivity) {
    let sensitivity = match &item {
        ClipboardItem::Text(text) => {
            if PrivacyGuard::classify(text).is_some() {
                Sensitivity::Sensitive
            } else {
                Sensitivity::Normal
            }
        }
        _ => Sensitivity::Normal,
    };

    (item, sensitivity)
}

fn enqueue_persistence_save(app_state: &Arc<AppState>) {
    let settings = app_state.settings.lock().unwrap().clone();

    if !settings.persist_history {
        return;
    }

    let items: Vec<StoredItem> = {
        let clipboard = app_state.clipboard.lock().unwrap();
        clipboard.items().cloned().collect()
    };

    let _ = app_state
        .persistence_tx
        .send(PersistenceCommand::SaveSnapshot { items, settings });
}

fn drain_persistence_events(app_state: &Arc<AppState>) {
    let mut events = Vec::new();

    {
        let rx = app_state.persistence_event_rx.lock().unwrap();

        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }
    }

    if events.is_empty() {
        return;
    }

    let mut latest_error = app_state.latest_error.lock().unwrap();

    for event in events {
        match event {
            PersistenceEvent::SaveFailed(error) => {
                *latest_error = Some(format!("Failed to save clipboard history: {}", error));
            }
            PersistenceEvent::SettingsSaveFailed(error) => {
                *latest_error = Some(format!("Failed to save settings: {}", error));
            }
            PersistenceEvent::DeleteFailed(error) => {
                *latest_error = Some(format!("Failed to delete stored history: {}", error));
            }
            PersistenceEvent::StoredHistoryDeleted => {
                *latest_error = Some(
                    "Stored history deleted. Future items may still be persisted if persistence remains enabled."
                        .into(),
                );
            }
        }
    }
}

fn process_actions(app_state: &Arc<AppState>, debouncer: &Arc<Mutex<SaveDebouncer>>) {
    let mut actions = Vec::new();

    {
        let rx = app_state.action_rx.lock().unwrap();

        while let Ok(action) = rx.try_recv() {
            actions.push(action);
        }
    }

    if actions.is_empty() {
        return;
    }

    let mut trigger_save = false;

    for action in actions {
        match action {
            AppAction::DeleteItem(id) => {
                let mut clipboard = app_state.clipboard.lock().unwrap();

                if clipboard.remove_by_id(id).is_some() {
                    trigger_save = true;
                }
            }
            AppAction::ClearInMemoryHistory => {
                app_state.clipboard.lock().unwrap().clear();
                trigger_save = true;
            }
            AppAction::DeleteStoredHistory => {
                let _ = app_state
                    .persistence_tx
                    .send(PersistenceCommand::DeleteStoredHistory);
            }
            AppAction::UpdateSettings(new_settings) => {
                app_state
                    .is_capturing
                    .store(new_settings.capture_enabled, Ordering::Relaxed);

                {
                    let mut clipboard = app_state.clipboard.lock().unwrap();
                    clipboard.reconfigure_limits(new_settings.max_items, new_settings.max_bytes);
                }

                *app_state.settings.lock().unwrap() = new_settings.clone();

                let _ = app_state
                    .persistence_tx
                    .send(PersistenceCommand::SaveSettings {
                        settings: new_settings.clone(),
                    });

                if new_settings.persist_history {
                    trigger_save = true;
                }
            }
            AppAction::CopyToClipboard(text) => match arboard::Clipboard::new() {
                Ok(mut clipboard) => {
                    if let Err(error) = clipboard.set_text(text.to_string()) {
                        *app_state.latest_error.lock().unwrap() =
                            Some(format!("Failed to write to OS clipboard: {}", error));
                    }
                }
                Err(error) => {
                    *app_state.latest_error.lock().unwrap() =
                        Some(format!("Failed to access OS clipboard: {}", error));
                }
            },
            AppAction::DismissError => {
                *app_state.latest_error.lock().unwrap() = None;
            }
        }
    }

    if trigger_save {
        enqueue_persistence_save(app_state);
        debouncer.lock().unwrap().mark_saved();
    }
}

fn drain_clipboard_channel(
    app_state: &Arc<AppState>,
    debouncer: &Arc<Mutex<SaveDebouncer>>,
) {
    let mut drained = Vec::new();

    {
        let rx = app_state.receiver.lock().unwrap();

        while let Ok(item) = rx.try_recv() {
            drained.push(item);
        }
    }

    if drained.is_empty() {
        return;
    }

    let settings = app_state.settings.lock().unwrap().clone();
    let mut received = false;

    {
        let mut clipboard = app_state.clipboard.lock().unwrap();
        let mut error_lock = app_state.latest_error.lock().unwrap();

        for result in drained {
            match result {
                Ok(raw_item) => {
                    let (item, sensitivity) = classify_item_for_memory(raw_item);

                    if sensitivity == Sensitivity::Sensitive
                        && settings.persist_history
                        && settings.filter_sensitive
                    {
                        *error_lock =
                            Some("Sensitive item kept in memory and excluded from persistence.".into());
                    }

                    clipboard.push(item, sensitivity);
                    received = true;
                }
                Err(error) => {
                    *error_lock = Some(error.user_message());
                }
            }
        }
    }

    if received && settings.persist_history {
        let mut debouncer = debouncer.lock().unwrap();

        if debouncer.should_save() {
            enqueue_persistence_save(app_state);
            debouncer.mark_saved();
        }
    }
}

fn build_system_tray() -> SystemTray {
    let show = CustomMenuItem::new("show".to_string(), "Show Clippt");
    let hide = CustomMenuItem::new("hide".to_string(), "Hide");
    let quit = CustomMenuItem::new("quit".to_string(), "Quit");

    let menu = SystemTrayMenu::new()
        .add_item(show)
        .add_item(hide)
        .add_native_item(tauri::SystemTrayMenuItem::Separator)
        .add_item(quit);

    SystemTray::new().with_menu(menu)
}

fn handle_tray_event(app: &tauri::AppHandle, event: SystemTrayEvent) {
    if let SystemTrayEvent::MenuItemClick { id, .. } = event {
        match id.as_str() {
            "show" => {
                if let Some(window) = app.get_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "hide" => {
                if let Some(window) = app.get_window("main") {
                    let _ = window.hide();
                }
            }
            "quit" => app.exit(0),
            _ => {}
        }
    }
}

fn main() -> tauri::Result<()> {
    env_logger::init();

    let (tx, rx) = mpsc::channel();
    let (action_tx, action_rx) = mpsc::channel();
    let (persistence_tx, persistence_rx) = mpsc::channel();
    let (persistence_event_tx, persistence_event_rx) = mpsc::channel();

    let is_capturing = Arc::new(AtomicBool::new(true));

    listener::spawn_clipboard_listener(tx, is_capturing.clone());

    let app_state = Arc::new(AppState {
        clipboard: Mutex::new(ClipboardState::new(100, 50 * 1024 * 1024)),
        receiver: Mutex::new(rx),
        action_rx: Mutex::new(action_rx),
        persistence_tx,
        persistence_event_rx: Mutex::new(persistence_event_rx),
        latest_error: Mutex::new(None),
        settings: Mutex::new(AppSettings::default()),
        is_capturing: is_capturing.clone(),
    });

    let save_debouncer = Arc::new(Mutex::new(SaveDebouncer::new()));

    tauri::Builder::default()
        .manage(app_state.clone())
        .manage(save_debouncer.clone())
        .plugin(tauri_plugin_egui::EguiPluginBuilder::default().build())
        .system_tray(build_system_tray())
        .on_system_tray_event(handle_tray_event)
        .setup(move |app| {
            let app_handle = app.handle();

            spawn_persistence_worker(app_handle.clone(), persistence_rx, persistence_event_tx);

            let settings = match load_settings(&app_handle) {
                Ok(settings) => settings,
                Err(error) => {
                    log::warn!("Could not load settings; using safe defaults: {}", error);
                    AppSettings::default()
                }
            };

            is_capturing.store(settings.capture_enabled, Ordering::Relaxed);

            {
                let mut clipboard = app_state.clipboard.lock().unwrap();
                clipboard.reconfigure_limits(settings.max_items, settings.max_bytes);
            }

            let persist_enabled = settings.persist_history;
            *app_state.settings.lock().unwrap() = settings;

            if persist_enabled {
                match load_state(&app_handle) {
                    Ok(loaded) => {
                        let count = loaded.len();
                        app_state.clipboard.lock().unwrap().restore_items(loaded);
                        log::info!("Loaded {} items from persistent storage.", count);
                    }
                    Err(error) => {
                        log::warn!("Could not load persistent state: {}", error);
                    }
                }
            } else {
                log::info!("Persistence disabled; stored history was not loaded.");
            }

            let window = tauri::WindowBuilder::new(
                app,
                "main",
                tauri::WindowUrl::App("index.html".into()),
            )
            .title("Clippt")
            .inner_size(900.0, 700.0)
            .visible(false)
            .build()?;

            let window_clone = window.clone();

            match app
                .global_shortcut_manager()
                .register("CmdOrCtrl+Shift+V", move || {
                    let visible = window_clone.is_visible().unwrap_or(false);

                    if visible {
                        let _ = window_clone.hide();
                    } else {
                        let _ = window_clone.show();
                        let _ = window_clone.set_focus();
                    }
                }) {
                Ok(_) => log::info!("Registered global shortcut CmdOrCtrl+Shift+V."),
                Err(error) => log::error!("Failed to register global shortcut: {}", error),
            }

            let ui_state = Arc::new(Mutex::new(ui::ClipptUi::new(
                app_state.clone(),
                action_tx,
            )));

            app.manage(ui_state.clone());

            let app_state_clone = app_state.clone();
            let debouncer_clone = save_debouncer.clone();

            window.start_egui(move |ctx| {
                process_actions(&app_state_clone, &debouncer_clone);
                drain_clipboard_channel(&app_state_clone, &debouncer_clone);
                drain_persistence_events(&app_state_clone);

                if let Ok(mut ui) = ui_state.lock() {
                    ui.update(ctx);
                }
            })?;

            Ok(())
        })
        .build(tauri::generate_context!())?
        .run(move |app_handle, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                let state = app_handle.state::<Arc<AppState>>();
                let settings = state.settings.lock().unwrap().clone();

                let (ack_tx, ack_rx) = mpsc::channel();

                if settings.clear_on_exit {
                    log::info!("Clear-on-exit enabled. Deleting stored history via worker.");
                    let _ = state
                        .persistence_tx
                        .send(PersistenceCommand::DeleteAndShutdown { ack: ack_tx });
                } else if settings.persist_history {
                    log::info!("Application exiting. Performing final state flush via worker.");

                    let items: Vec<StoredItem> = {
                        let clipboard = state.clipboard.lock().unwrap();
                        clipboard.items().cloned().collect()
                    };

                    let _ = state.persistence_tx.send(PersistenceCommand::SaveAndShutdown {
                        items,
                        settings,
                        ack: ack_tx,
                    });
                } else {
                    log::info!("Persistence disabled; shutting down worker.");
                    let _ = state
                        .persistence_tx
                        .send(PersistenceCommand::Shutdown { ack: ack_tx });
                }

                if wait_for_worker_ack(ack_rx, Duration::from_secs(3)).is_err() {
                    log::warn!("Persistence worker did not acknowledge shutdown before timeout.");
                }
            }
        });

    Ok(())
}
