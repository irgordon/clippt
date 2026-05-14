<div align="center">
  <h1>Clippt</h1>
  <img src="https://img.shields.io/badge/Rust-000000?style=for-the-badge&logo=rust&logoColor=aa2704" alt="Rust" />
  <img src="https://img.shields.io/badge/Tauri-FFC131?style=for-the-badge&logo=tauri&logoColor=white" alt="Tauri" />
  <img src="https://img.shields.io/badge/Web_UI-HTML%2FCSS%2FJS-42b983?style=for-the-badge" alt="Web UI" />
</div>

<br />

Clippt is a local-first clipboard manager built with Rust and a minimal Tauri web UI. It is designed to keep clipboard history on the local machine, with Rust remaining authoritative for clipboard capture, privacy filtering, settings, persistence, and clipboard actions.

## Current status

Clippt is under production-readiness hardening. 


## Features

* **Local-first operation:** Clippt runs on your machine without telemetry, tracking scripts, mandatory accounts, cloud sync, or network behavior.
* **Best-effort sensitive filtering:** Clippt uses lightweight heuristics for common high-risk text patterns such as private keys, API-key-shaped tokens, and JWT-shaped strings. This reduces accidental persistence risk but is not complete secret detection.
* **Bounded memory guardrails:** Clippt enforces item-count and byte limits for in-memory history and evicts older entries when configured limits require it.
* **Minimal runtime UI:** The framework-free Tauri web UI renders capture status, persistence status, sensitive-filter status, settings, and clipboard entry previews.
* **Opt-in persistence:** History persistence is disabled by default. If enabled, Rust decides persistence eligibility from current settings and sensitivity metadata.

## Installation

Signed and notarized production installers are not yet claimed ready. Until release validation is complete, build and test locally from source.

```bash
cd src-tauri
cargo tauri build
```

## Configuration

Use the settings panel to control Clippt's data footprint:

* Toggle clipboard capture on or off.
* Enable or disable history persistence.
* Choose whether text, images, and file paths may be persisted.
* Keep best-effort sensitive filtering enabled or disable it explicitly.
* Set capacity limits for item count and memory bytes.
* Clear in-memory history or delete stored history.

Deleting stored history does not disable future persistence if persistence remains enabled.

## Architecture

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the Path B web UI architecture and authority-boundary notes.

## Contributing

Clippt is an open-source project. If you are interested in improving the application, feel free to fork the repository and submit a pull request.
