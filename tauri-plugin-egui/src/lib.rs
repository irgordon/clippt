//! Compatibility shim for Clippt's current Tauri v1 + egui integration.
//!
//! See `../SHIM_NOTES.md` for scope, API surface, and removal criteria.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{plugin::TauriPlugin, Runtime, WindowEvent};

const FRAME_INTERVAL: Duration = Duration::from_millis(16);

enum LoopSignal {
    RepaintAfter(Duration),
    Stop,
}

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
        let window = self.clone();
        let label = window.label().to_string();
        let (signal_tx, signal_rx) = mpsc::channel();
        let event_tx = signal_tx.clone();

        window.on_window_event(move |event| {
            if matches!(event, WindowEvent::CloseRequested { .. } | WindowEvent::Destroyed) {
                let _ = event_tx.send(LoopSignal::Stop);
            }
        });

        thread::Builder::new()
            .name(format!("egui-bridge-{label}"))
            .spawn(move || run_egui_bridge(window, render, signal_tx, signal_rx))
            .expect("failed to spawn tauri-plugin-egui bridge thread");

        Ok(())
    }
}

fn run_egui_bridge<R, F>(
    window: tauri::Window<R>,
    mut render: F,
    signal_tx: Sender<LoopSignal>,
    signal_rx: Receiver<LoopSignal>,
) where
    R: Runtime,
    F: FnMut(&egui::Context) + Send + 'static,
{
    let ctx = egui::Context::default();
    let repaint_tx = signal_tx.clone();
    let window_label = window.label().to_string();
    let started_at = Instant::now();

    ctx.set_request_repaint_callback(move |info| {
        let _ = repaint_tx.send(LoopSignal::RepaintAfter(info.after));
    });

    log::info!(
        "tauri-plugin-egui bridge loop started for window `{}`; callback scheduling is active.",
        window_label
    );

    loop {
        let raw_input = build_raw_input(&window, started_at.elapsed());
        let output = ctx.run(raw_input, |ctx| render(ctx));
        let wait_for = output.repaint_after.min(FRAME_INTERVAL);

        if !wait_for_next_frame(&signal_rx, wait_for) {
            break;
        }
    }
}

fn wait_for_next_frame(signal_rx: &Receiver<LoopSignal>, wait_for: Duration) -> bool {
    let mut deadline = Instant::now() + wait_for;

    loop {
        let now = Instant::now();

        if now >= deadline {
            return true;
        }

        match signal_rx.recv_timeout(deadline.saturating_duration_since(now)) {
            Ok(LoopSignal::Stop) => return false,
            Ok(LoopSignal::RepaintAfter(after)) => {
                let requested = Instant::now() + after;
                if requested < deadline {
                    deadline = requested;
                }
            }
            Err(RecvTimeoutError::Timeout) => return true,
            Err(RecvTimeoutError::Disconnected) => return false,
        }
    }
}

fn build_raw_input<R: Runtime>(window: &tauri::Window<R>, elapsed: Duration) -> egui::RawInput {
    let inner_size = window.inner_size().ok();
    let scale_factor = window.scale_factor().ok().filter(|factor| *factor > 0.0);
    let pixels_per_point = scale_factor.map(|factor| factor as f32);

    let screen_rect = inner_size.map(|size| {
        let pixels_per_point = pixels_per_point.unwrap_or(1.0);
        let size = egui::vec2(
            size.width as f32 / pixels_per_point,
            size.height as f32 / pixels_per_point,
        );
        egui::Rect::from_min_size(egui::Pos2::ZERO, size)
    });

    egui::RawInput {
        screen_rect: screen_rect.or_else(|| {
            Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 700.0),
            ))
        }),
        pixels_per_point,
        time: Some(elapsed.as_secs_f64()),
        ..Default::default()
    }
}
