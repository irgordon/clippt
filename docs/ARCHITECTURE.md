# Clippt Architecture Specification

## Overview

Clippt is a high-performance, cross-platform clipboard manager built with Rust and Tauri v1. The runtime UI is now a minimal Tauri web frontend because the local `tauri-plugin-egui` crate was only a compile-time compatibility shim: it accepted `start_egui(...)`, did not invoke a real renderer, did not paint pixels, did not handle input, and did not integrate with the Tauri window lifecycle.

The abandoned egui bridge was not replaced with a custom native renderer because a correct Tauri v1 + egui path would need ownership of an `egui::Context`, input translation, output handling, tessellation, a painter/renderer, repaint scheduling, and lifecycle integration. The available upstream `tauri-egui` crate provides a real glutin-backed egui window, but its maintained line targets Tauri alpha APIs and the older Tauri v1-compatible release uses a different `eframe` window API and older egui versions rather than Clippt's `window.start_egui(...)` boundary. Rebuilding that bridge locally would be a fragile graphics integration for this recovery pass.

## UI architecture decision

Clippt uses Path B: a minimal Tauri web UI backed by Rust commands.

- Rust remains authoritative for clipboard capture, state mutation, settings, privacy classification, persistence, stored-history deletion, and OS clipboard writes.
- The frontend is non-authoritative. It renders snapshots returned by `get_app_snapshot` and sends user intent through typed Tauri commands.
- Tauri commands enqueue `AppAction` values instead of directly writing persistence files.
- A Rust controller loop in `main.rs` drains `AppAction`, clipboard listener messages, and persistence worker events away from the web render path.
- The web UI polls state snapshots for display and does not perform disk I/O.

## Core architecture

The application follows a unidirectional data flow. Clipboard capture, in-memory state, privacy classification, persistence, and rendering are separated so expensive operating-system and disk operations do not block UI rendering.

### 1. OS clipboard listener (`listener.rs`)

**Role:** A background thread dedicated to polling the host operating system clipboard.

**Characteristics:**

- Uses exponential backoff for resilient clipboard initialization.
- Can be paused through a lock-free `Arc<AtomicBool>` before polling operating-system clipboard APIs.
- Sends clipboard observations to the controller through `mpsc::Sender`.
- Stores text payloads as `Arc<str>` before handing them to the rest of the application.
- Uses metadata to suppress repeated oversized-image errors, but hashes accepted-size image payloads so different images with the same dimensions and byte length are not skipped.

### 2. State management (`state.rs`)

**Role:** The authoritative bounded in-memory clipboard history.

**Characteristics:**

- Enforces strict limits on item count and total byte capacity.
- Assigns stable `u64` item identifiers and advances `next_id` across restore so item identity does not collide after restart.
- Stores text as `Arc<str>` and images as `Arc<Vec<u8>>`, allowing snapshots to clone references instead of copying large payloads internally.
- Preserves sensitivity metadata on stored items.
- Rejects single items that exceed the configured memory budget.

### 3. Privacy and settings (`privacy.rs`, `settings.rs`)

**Role:** Classifies captured content and records user persistence intent.

**Characteristics:**

- Uses a two-gate model: capture to memory and persistence to disk are separate decisions.
- Stores sensitivity as item metadata at capture time.
- Applies persistence eligibility dynamically at save time through `should_persist_item`, so settings changes affect future persistence decisions without rewriting captured items.
- Provides lightweight best-effort heuristic scanners for common sensitive text patterns, including private key blocks, AWS access key IDs, JWT-shaped strings, GitHub tokens, and OpenAI-style API keys.
- Does not claim complete secret detection. The scanner reduces accidental persistence risk but cannot prove that arbitrary text is safe.

### 4. Persistence worker (`persistence_worker.rs`, `persistence.rs`)

**Role:** A dedicated background thread for disk I/O.

**Characteristics:**

- Keeps history and settings writes off the UI render path.
- Receives persistence commands over a worker channel.
- Reports persistence failures back to the controller through `PersistenceEvent`.
- Writes JSON and binary image files to process-scoped temporary files, syncs file contents, and renames them into place. This prevents partial final-file writes in normal crash scenarios and reduces persistence corruption risk.
- Cleans stale `.clippt_*.tmp` files.
- Sweeps orphaned `clipimg_*.bin` files after successful state writes.
- Provides directory-level persistence functions for testability, with `AppHandle` wrappers used by the application runtime.

### 5. Controller, commands, and frontend (`main.rs`, `action.rs`, `dist/index.html`)

**Role:** The Rust controller owns mutation; the frontend displays snapshots and emits intent.

**Characteristics:**

- The frontend reads `AppSnapshot` values through `get_app_snapshot`.
- User intent is sent through Tauri commands and converted to `AppAction` values.
- Clipboard deletion, clearing, settings updates, stored-history deletion, and copy-to-clipboard operations are routed through Rust.
- Render-path persistence work is limited to enqueueing worker commands and draining worker events from the Rust controller loop.
- The frontend intentionally stays framework-free and small; it is not the source of truth for settings, privacy classification, persistence, or clipboard contents.
