use crate::settings::AppSettings;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub enum AppAction {
    DeleteItem(u64),
    ClearInMemoryHistory,
    DeleteStoredHistory,
    UpdateSettings(AppSettings),
    CopyToClipboard(Arc<str>),
    DismissError,
}
