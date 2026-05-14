use crate::state::{ClipboardItem, ClipptError};
use arboard::Clipboard;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc::Sender, Arc};
use std::thread;
use std::time::Duration;

const MAX_IMAGE_SIZE_BYTES: usize = 10 * 1024 * 1024;

fn calculate_hash<T: Hash>(value: &T) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

pub fn spawn_clipboard_listener(
    tx: Sender<Result<ClipboardItem, ClipptError>>,
    is_active: Arc<AtomicBool>,
) {
    thread::spawn(move || {
        let mut backoff = 1u64;
        let mut first_error_sent = false;

        let mut clipboard = loop {
            match Clipboard::new() {
                Ok(clipboard) => break clipboard,
                Err(_) => {
                    if !first_error_sent {
                        let _ = tx.send(Err(ClipptError::OsClipboardAccessFailed));
                        first_error_sent = true;
                    }

                    thread::sleep(Duration::from_secs(backoff));
                    backoff = (backoff * 2).min(8);
                }
            }
        };

        let mut last_text_hash: Option<u64> = None;
        let mut last_image_hash: Option<u64> = None;
        let mut last_rejected_image_meta: Option<(usize, usize, usize)> = None;

        loop {
            thread::sleep(Duration::from_millis(300));

            if !is_active.load(Ordering::Relaxed) {
                continue;
            }

            if let Ok(text) = clipboard.get_text() {
                let text = text.trim().to_string();

                if !text.is_empty() {
                    let hash = calculate_hash(&text);

                    if Some(hash) != last_text_hash {
                        last_text_hash = Some(hash);

                        if let Some(path) = parse_file_url(&text) {
                            let _ = tx.send(Ok(ClipboardItem::File(path)));
                        } else {
                            let _ = tx.send(Ok(ClipboardItem::Text(Arc::<str>::from(text))));
                        }
                    }
                }
            }

            if let Ok(image_data) = clipboard.get_image() {
                let width = image_data.width;
                let height = image_data.height;
                let len = image_data.bytes.len();

                if len == 0 {
                    continue;
                }

                let meta = (width, height, len);

                if len > MAX_IMAGE_SIZE_BYTES {
                    if Some(meta) != last_rejected_image_meta {
                        last_rejected_image_meta = Some(meta);
                        let _ = tx.send(Err(ClipptError::ItemTooLarge));
                    }

                    continue;
                }

                let bytes = image_data.bytes.into_owned();
                let hash = calculate_hash(&bytes);

                if Some(hash) != last_image_hash {
                    last_image_hash = Some(hash);
                    let _ = tx.send(Ok(ClipboardItem::Image(width, height, Arc::new(bytes))));
                }
            }
        }
    });
}

fn parse_file_url(text: &str) -> Option<PathBuf> {
    let trimmed = text.trim();

    if !trimmed.starts_with("file://") {
        return None;
    }

    let without_scheme = trimmed.trim_start_matches("file://");

    if without_scheme.is_empty() {
        return None;
    }

    let path = if cfg!(target_os = "windows") {
        without_scheme.trim_start_matches('/')
    } else {
        without_scheme
    };

    let path = percent_decode_minimal(path);
    let path = PathBuf::from(path);

    if path.exists() {
        Some(path)
    } else {
        None
    }
}

fn percent_decode_minimal(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) =
                (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
            {
                output.push((high << 4) | low);
                index += 3;
                continue;
            }
        }

        output.push(bytes[index]);
        index += 1;
    }

    String::from_utf8_lossy(&output).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
