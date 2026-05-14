use crate::settings::AppSettings;

#[derive(Clone, Debug)]
pub enum AppAction {
    DeleteItem(u64),
    ClearInMemoryHistory,
    DeleteStoredHistory,
    UpdateSettings(AppSettings),
    CopyTextItem(u64),
    DismissError,
}
