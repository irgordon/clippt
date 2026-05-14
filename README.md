<div align="center">
  <h1>Clippt</h1>
  <img src="https://img.shields.io/badge/Rust-000000?style=for-the-badge&logo=rust&logoColor=aa2704" alt="Rust" />
  <img src="https://img.shields.io/badge/Tauri-FFC131?style=for-the-badge&logo=tauri&logoColor=white" alt="Tauri" />
  <img src="https://img.shields.io/badge/egui-42b983?style=for-the-badge&logo=rust&logoColor=000000" alt="egui" />
</div>

<br />

Your clipboard history belongs to you. Keep it that way.

Clippt is a modern, local-first clipboard manager built from the ground up for speed, stability, and absolute privacy. Designed to run silently in the background, it remembers what you copy so you never lose your train of thought, while ensuring your most sensitive data never leaves your control.

## Features

* **Private by Design:** Clippt runs entirely on your local machine. There are no tracking scripts, no telemetry, and no mandatory cloud accounts.
* **Smart Sensitive Data Filtering:** Clippt automatically detects sensitive information like API keys, private certificates, and authentication tokens. It holds them in memory for immediate use but refuses to write them to your hard drive.
* **Bounded Memory Guardrails:** Copying massive images or files will not slow down your computer. Clippt strictly manages its own memory footprint, quietly removing the oldest items only when necessary to stay within the limits you set.
* **Lightning Fast Search:** Instantly retrieve past text, images, and file paths with an optimized search engine that never lags, even with thousands of items in your history.
* **Opt-In Persistence:** You decide what gets saved. Run Clippt entirely in memory for a session-based workflow, or enable disk persistence to keep your history across reboots.

## Installation

Download the latest native installer for macOS, Windows, or Linux from the Releases page.

1. Install the application.
2. Launch Clippt.
3. Use your system's global shortcut to bring up your clipboard history from anywhere.

## Configuration

Clippt puts you in control of your data footprint. Access the settings panel to tailor your experience:
* Toggle clipboard capture on or off at any time.
* Enable or disable history persistence.
* Set exact capacity limits for items and disk space.
* Clear your memory or delete your stored data with a single click.

## Contributing

Clippt is an open-source project. If you are interested in improving the application, feel free to fork the repository and submit a pull request.
