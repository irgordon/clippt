use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub enum ClipboardItem {
    Text(Arc<str>),
    Image(usize, usize, Arc<Vec<u8>>),
    File(PathBuf),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sensitivity {
    Normal,
    Sensitive,
}

#[derive(Clone, Debug)]
pub struct StoredItem {
    pub id: u64,
    pub item: ClipboardItem,
    pub sensitivity: Sensitivity,
}

impl StoredItem {
    pub fn approx_size(&self) -> usize {
        match &self.item {
            ClipboardItem::Text(text) => text.len(),
            ClipboardItem::Image(_, _, bytes) => bytes.len(),
            ClipboardItem::File(path) => path.to_string_lossy().len(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum ClipptError {
    OsClipboardAccessFailed,
    ItemTooLarge,
}

impl ClipptError {
    pub fn user_message(&self) -> String {
        match self {
            Self::OsClipboardAccessFailed => {
                "Could not read clipboard. Check operating system permissions.".into()
            }
            Self::ItemTooLarge => "Item too large to save to clipboard history.".into(),
        }
    }
}

pub struct ClipboardState {
    history: VecDeque<StoredItem>,
    max_items: usize,
    max_bytes: usize,
    current_bytes: usize,
    next_id: u64,
}

impl ClipboardState {
    pub fn new(max_items: usize, max_bytes: usize) -> Self {
        Self {
            history: VecDeque::with_capacity(max_items.max(1)),
            max_items: max_items.max(1),
            max_bytes: max_bytes.max(1),
            current_bytes: 0,
            next_id: 1,
        }
    }

    pub fn push(&mut self, item: ClipboardItem, sensitivity: Sensitivity) {
        let stored = StoredItem {
            id: self.next_id,
            item,
            sensitivity,
        };

        self.next_id = self.next_id.saturating_add(1);

        let item_size = stored.approx_size();

        if item_size > self.max_bytes {
            log::warn!("Dropped item larger than max_bytes: {} bytes", item_size);
            return;
        }

        self.evict_until_fits(item_size);

        self.current_bytes = self.current_bytes.saturating_add(item_size);
        self.history.push_back(stored);
    }

    pub fn restore_items(&mut self, items: Vec<StoredItem>) {
        self.history.clear();
        self.current_bytes = 0;

        let mut max_id = 0;

        for stored in items {
            max_id = max_id.max(stored.id);
            let item_size = stored.approx_size();

            if item_size > self.max_bytes {
                log::warn!(
                    "Persisted item id={} exceeds current max_bytes and was discarded.",
                    stored.id
                );
                continue;
            }

            self.evict_until_fits(item_size);

            self.current_bytes = self.current_bytes.saturating_add(item_size);
            self.history.push_back(stored);
        }

        self.next_id = max_id.saturating_add(1).max(1);

        log::info!(
            "State restored: {} items, {} bytes. Next ID starts at {}.",
            self.history.len(),
            self.current_bytes,
            self.next_id
        );
    }

    pub fn reconfigure_limits(&mut self, max_items: usize, max_bytes: usize) {
        self.max_items = max_items.max(1);
        self.max_bytes = max_bytes.max(1);

        while self.history.len() > self.max_items || self.current_bytes > self.max_bytes {
            if let Some(evicted) = self.history.pop_front() {
                self.current_bytes = self.current_bytes.saturating_sub(evicted.approx_size());
                log::debug!(
                    "Evicted item id={} during limit reconfiguration.",
                    evicted.id
                );
            } else {
                break;
            }
        }
    }

    pub fn items(&self) -> impl Iterator<Item = &StoredItem> {
        self.history.iter()
    }

    pub fn len(&self) -> usize {
        self.history.len()
    }

    pub fn current_bytes(&self) -> usize {
        self.current_bytes
    }

    pub fn remove_by_id(&mut self, id: u64) -> Option<StoredItem> {
        if let Some(pos) = self.history.iter().position(|item| item.id == id) {
            let removed = self.history.remove(pos).unwrap();
            self.current_bytes = self.current_bytes.saturating_sub(removed.approx_size());

            log::debug!(
                "Removed item id={} reclaimed={} bytes.",
                removed.id,
                removed.approx_size()
            );

            return Some(removed);
        }

        None
    }

    pub fn clear(&mut self) {
        self.history.clear();
        self.current_bytes = 0;
        log::info!("In-memory clipboard state cleared.");
    }

    fn evict_until_fits(&mut self, incoming_size: usize) {
        while !self.history.is_empty()
            && (self.current_bytes.saturating_add(incoming_size) > self.max_bytes
                || self.history.len() >= self.max_items)
        {
            if let Some(evicted) = self.history.pop_front() {
                let reclaimed = evicted.approx_size();
                self.current_bytes = self.current_bytes.saturating_sub(reclaimed);

                log::debug!(
                    "Evicted item id={} reclaimed={} bytes.",
                    evicted.id,
                    reclaimed
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_item(id: u64, content: &str) -> StoredItem {
        StoredItem {
            id,
            item: ClipboardItem::Text(Arc::<str>::from(content)),
            sensitivity: Sensitivity::Normal,
        }
    }

    #[test]
    fn restore_clears_previous_state() {
        let mut state = ClipboardState::new(10, 1024);
        state.push(
            ClipboardItem::Text(Arc::<str>::from("stale")),
            Sensitivity::Normal,
        );

        state.restore_items(vec![text_item(99, "fresh")]);

        let items: Vec<_> = state.items().collect();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, 99);
    }

    #[test]
    fn restore_preserves_ids_and_advances_next_id() {
        let mut state = ClipboardState::new(10, 1024);

        state.restore_items(vec![text_item(5, "hello"), text_item(12, "world")]);

        let items: Vec<_> = state.items().collect();
        assert_eq!(items[0].id, 5);
        assert_eq!(items[1].id, 12);
        assert_eq!(state.next_id, 13);
    }

    #[test]
    fn restore_advances_next_id_past_discarded_high_id() {
        let mut state = ClipboardState::new(10, 5);

        state.restore_items(vec![text_item(1, "ok"), text_item(99, "too-large")]);

        let items: Vec<_> = state.items().collect();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, 1);
        assert_eq!(state.next_id, 100);
    }

    #[test]
    fn restore_recalculates_current_bytes() {
        let mut state = ClipboardState::new(10, 1024);

        state.restore_items(vec![text_item(1, "12345"), text_item(2, "67890")]);

        assert_eq!(state.current_bytes(), 10);
    }

    #[test]
    fn restore_discards_oversized_items() {
        let mut state = ClipboardState::new(10, 5);

        state.restore_items(vec![
            text_item(1, "123"),
            text_item(2, "123456"),
            text_item(3, "12"),
        ]);

        let items: Vec<_> = state.items().collect();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].id, 1);
        assert_eq!(items[1].id, 3);
        assert_eq!(state.current_bytes(), 5);
        assert_eq!(state.next_id, 4);
    }

    #[test]
    fn restore_evicts_oldest_when_count_limit_exceeded() {
        let mut state = ClipboardState::new(2, 1024);

        state.restore_items(vec![
            text_item(1, "A"),
            text_item(2, "B"),
            text_item(3, "C"),
        ]);

        let items: Vec<_> = state.items().collect();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].id, 2);
        assert_eq!(items[1].id, 3);
    }

    #[test]
    fn restore_evicts_oldest_when_byte_limit_exceeded() {
        let mut state = ClipboardState::new(10, 10);

        state.restore_items(vec![
            text_item(1, "12345"),
            text_item(2, "123456"),
            text_item(3, "1234"),
            text_item(4, "1"),
        ]);

        let items: Vec<_> = state.items().collect();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].id, 3);
        assert_eq!(items[1].id, 4);
        assert_eq!(state.current_bytes(), 5);
    }

    #[test]
    fn restore_preserves_sensitivity() {
        let mut state = ClipboardState::new(10, 1024);

        state.restore_items(vec![StoredItem {
            id: 7,
            item: ClipboardItem::Text(Arc::<str>::from("secret")),
            sensitivity: Sensitivity::Sensitive,
        }]);

        let items: Vec<_> = state.items().collect();
        assert_eq!(items[0].sensitivity, Sensitivity::Sensitive);
    }

    #[test]
    fn approx_size_image_and_file() {
        let file_item = StoredItem {
            id: 1,
            item: ClipboardItem::File(PathBuf::from("/etc/passwd")),
            sensitivity: Sensitivity::Normal,
        };

        assert_eq!(file_item.approx_size(), 11);

        let image_item = StoredItem {
            id: 2,
            item: ClipboardItem::Image(10, 10, Arc::new(vec![0; 400])),
            sensitivity: Sensitivity::Normal,
        };

        assert_eq!(image_item.approx_size(), 400);
    }

    #[test]
    fn clear_resets_history_and_bytes_without_regressing_ids() {
        let mut state = ClipboardState::new(10, 1024);

        state.push(
            ClipboardItem::Text(Arc::<str>::from("Data")),
            Sensitivity::Normal,
        );

        assert_eq!(state.current_bytes(), 4);

        state.clear();

        assert_eq!(state.items().count(), 0);
        assert_eq!(state.current_bytes(), 0);
        assert_eq!(state.next_id, 2);
    }

    #[test]
    fn reconfigure_limits_evicts_to_new_bounds() {
        let mut state = ClipboardState::new(10, 1024);

        state.push(
            ClipboardItem::Text(Arc::<str>::from("12345")),
            Sensitivity::Normal,
        );
        state.push(
            ClipboardItem::Text(Arc::<str>::from("67890")),
            Sensitivity::Normal,
        );
        state.push(
            ClipboardItem::Text(Arc::<str>::from("abcde")),
            Sensitivity::Normal,
        );

        state.reconfigure_limits(2, 10);

        let items: Vec<_> = state.items().collect();
        assert_eq!(items.len(), 2);
        assert_eq!(state.current_bytes(), 10);
        assert_eq!(items[0].id, 2);
        assert_eq!(items[1].id, 3);
    }
}
