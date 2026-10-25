//! Windows has no supported Codex TUI daemon origin discovery yet.
pub(crate) fn daemon_tui_origin_for(
    _thread: &str,
    _home: &std::path::Path,
    _env: &std::collections::HashMap<String, String>,
) -> Option<String> {
    None
}
