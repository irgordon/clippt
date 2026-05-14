use crate::persistence::{delete_persisted_history, save_state};
use crate::settings::{save_settings, AppSettings};
use crate::state::StoredItem;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};
use tauri::AppHandle;

#[derive(Debug)]
pub enum PersistenceCommand {
    SaveSnapshot {
        items: Vec<StoredItem>,
        settings: AppSettings,
    },
    SaveSettings {
        settings: AppSettings,
    },
    DeleteStoredHistory,
    SaveAndShutdown {
        items: Vec<StoredItem>,
        settings: AppSettings,
        ack: Sender<()>,
    },
    DeleteAndShutdown {
        ack: Sender<()>,
    },
    Shutdown {
        ack: Sender<()>,
    },
}

#[derive(Debug)]
pub enum PersistenceEvent {
    SaveFailed(String),
    SettingsSaveFailed(String),
    DeleteFailed(String),
    StoredHistoryDeleted,
}

pub fn spawn_persistence_worker(
    app_handle: AppHandle,
    rx: Receiver<PersistenceCommand>,
    event_tx: Sender<PersistenceEvent>,
) {
    thread::spawn(move || {
        let mut last_save_failed_at: Option<Instant> = None;

        while let Ok(command) = rx.recv() {
            match command {
                PersistenceCommand::SaveSnapshot { items, settings } => {
                    if settings.persist_history {
                        if let Err(error) = save_state(&app_handle, &items, &settings) {
                            let now = Instant::now();

                            if last_save_failed_at
                                .map(|last| now.duration_since(last) > Duration::from_secs(5))
                                .unwrap_or(true)
                            {
                                let _ = event_tx.send(PersistenceEvent::SaveFailed(error.to_string()));
                                last_save_failed_at = Some(now);
                            }
                        }
                    }
                }
                PersistenceCommand::SaveSettings { settings } => {
                    if let Err(error) = save_settings(&app_handle, &settings) {
                        let _ = event_tx.send(PersistenceEvent::SettingsSaveFailed(error.to_string()));
                    }
                }
                PersistenceCommand::DeleteStoredHistory => match delete_persisted_history(&app_handle) {
                    Ok(_) => {
                        let _ = event_tx.send(PersistenceEvent::StoredHistoryDeleted);
                    }
                    Err(error) => {
                        let _ = event_tx.send(PersistenceEvent::DeleteFailed(error.to_string()));
                    }
                },
                PersistenceCommand::SaveAndShutdown {
                    items,
                    settings,
                    ack,
                } => {
                    if settings.persist_history {
                        if let Err(error) = save_state(&app_handle, &items, &settings) {
                            let _ = event_tx.send(PersistenceEvent::SaveFailed(error.to_string()));
                        }
                    }

                    let _ = ack.send(());
                    break;
                }
                PersistenceCommand::DeleteAndShutdown { ack } => {
                    if let Err(error) = delete_persisted_history(&app_handle) {
                        let _ = event_tx.send(PersistenceEvent::DeleteFailed(error.to_string()));
                    }

                    let _ = ack.send(());
                    break;
                }
                PersistenceCommand::Shutdown { ack } => {
                    let _ = ack.send(());
                    break;
                }
            }
        }
    });
}

pub fn wait_for_worker_ack(
    rx: mpsc::Receiver<()>,
    timeout: Duration,
) -> Result<(), RecvTimeoutError> {
    rx.recv_timeout(timeout)
}
