# Clippt Architecture Specification

## Overview

Clippt is a high-performance, cross-platform clipboard manager built with Rust, Tauri, and egui. It is designed around strict domain boundaries, bounded memory use, non-blocking UI rendering, and user-controlled privacy.

## Core architecture

The application follows a unidirectional data flow. Clipboard capture, in-memory state, privacy classification, persistence, and rendering are separated so expensive operating-system and disk operations do not block the egui render path.

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
- Stores text as `Arc<str>` and images as `Arc<Vec<u8>>`, allowing render snapshots to clone references instead of copying large payloads.
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

- Keeps history and settings writes off the egui render path.
- Receives persistence commands over a worker channel.
- Reports persistence failures back to the controller through `PersistenceEvent`.
- Writes JSON and binary image files to process-scoped temporary files, syncs file contents, and renames them into place. This prevents partial final-file writes in normal crash scenarios and reduces persistence corruption risk.
- Cleans stale `.clippt_*.tmp` files.
- Sweeps orphaned `clipimg_*.bin` files after successful state writes.
- Provides directory-level persistence functions for testability, with `AppHandle` wrappers used by the application runtime.

### 5. Render and command loop (`main.rs`, `ui.rs`, `action.rs`)

**Role:** The immediate-mode frontend and central controller.

**Characteristics:**

- The UI may read state snapshots for rendering, but it does not directly mutate `ClipboardState`.
- User intent is emitted as `AppAction` values and processed by the controller.
- Clipboard deletion, clearing, settings updates, and copy-to-clipboard operations are routed through the controller.
- Render-loop persistence work is limited to enqueueing worker commands and draining worker events.
- Search performs allocation-free ASCII case-insensitive matching directly over byte windows. For non-ASCII text, it uses exact matching rather than allocating lowercase copies per frame.
