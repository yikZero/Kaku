//! Persistent state for the AI chat overlay.
//!
//! Stores UI state like the last selected model in `~/.config/kaku/ai_chat_state.json`.
//! Load once at overlay start; save when the user switches models.

use anyhow::{Context, Result};
use std::path::PathBuf;

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct StateFile {
    version: u32,
    /// Last model selected by the user via Shift+Tab.
    last_model: Option<String>,
    /// Cached model list from the last successful /models fetch.
    #[serde(default)]
    cached_models: Vec<String>,
}

/// Load the last selected model from disk. Returns None on any error (non-fatal).
pub fn load_last_model() -> Option<String> {
    try_load().ok().flatten().and_then(|f| f.last_model)
}

/// Load the cached model list from the previous session.
pub fn load_cached_models() -> Vec<String> {
    try_load()
        .ok()
        .flatten()
        .map(|f| f.cached_models)
        .unwrap_or_default()
}

fn try_load() -> Result<Option<StateFile>> {
    let path = state_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let file: StateFile =
        serde_json::from_str(&raw).with_context(|| format!("parse {}", path.display()))?;
    Ok(Some(file))
}

fn load_or_default() -> StateFile {
    try_load()
        .unwrap_or_else(|e| {
            log::warn!("Could not load AI chat state: {e}");
            None
        })
        .unwrap_or_default()
}

/// Save the last selected model to disk atomically.
pub fn save_last_model(model: &str) -> Result<()> {
    let path = state_path()?;
    let mut file = load_or_default();
    file.version = 1;
    file.last_model = Some(model.to_string());
    write_state(&path, &file)
}

/// Persist the fetched model list so the next session starts without a loading delay.
pub fn save_cached_models(models: &[String]) -> Result<()> {
    let path = state_path()?;
    let mut file = load_or_default();
    file.version = 1;
    file.cached_models = models.to_vec();
    write_state(&path, &file)
}

fn write_state(path: &std::path::PathBuf, file: &StateFile) -> Result<()> {
    let json = serde_json::to_string_pretty(file).context("serialize state")?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, &json).with_context(|| format!("write {}", tmp.display()))?;
    std::fs::rename(&tmp, path)
        .with_context(|| format!("rename {} -> {}", tmp.display(), path.display()))?;
    Ok(())
}

fn state_path() -> Result<PathBuf> {
    let user_config_path = config::user_config_path();
    let config_dir = user_config_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("invalid user config path"))?;
    Ok(config_dir.join("ai_chat_state.json"))
}
