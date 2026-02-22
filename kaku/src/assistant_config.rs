use anyhow::{anyhow, Context};
use std::path::{Path, PathBuf};

pub const DEFAULT_MODEL: &str = "gpt-5-mini";
pub const DEFAULT_BASE_URL: &str = "https://api.vivgrid.com/v1";

pub fn assistant_toml_path() -> anyhow::Result<PathBuf> {
    let user_config_path = config::user_config_path();
    let config_dir = user_config_path
        .parent()
        .ok_or_else(|| anyhow!("invalid user config path: {}", user_config_path.display()))?;
    Ok(config_dir.join("assistant.toml"))
}

pub fn ensure_assistant_toml_exists() -> anyhow::Result<PathBuf> {
    let path = assistant_toml_path()?;
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("invalid assistant.toml path: {}", path.display()))?;
    config::create_user_owned_dirs(parent).context("create config directory")?;

    if !path.exists() {
        std::fs::write(&path, default_assistant_toml_template())
            .with_context(|| format!("write {}", path.display()))?;
    }

    ensure_required_keys(&path)?;
    Ok(path)
}

pub fn default_assistant_toml_template() -> String {
    format!(
        "# Kaku Assistant configuration\n\
# enabled: true enables command analysis suggestions; false disables requests.\n\
# api_key: provider API key, example: \"sk-xxxx\".\n\
# model: model id, example: \"DeepSeek-V3.2\" or \"gpt-5-mini\".\n\
# base_url: chat-completions API root URL.\n\
\n\
enabled = true\n\
# api_key = \"<your_api_key>\"\n\
model = \"{DEFAULT_MODEL}\"\n\
base_url = \"{DEFAULT_BASE_URL}\"\n"
    )
}

fn ensure_required_keys(path: &Path) -> anyhow::Result<()> {
    let raw = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let mut updated = raw.trim_end().to_string();
    let mut changed = false;

    if !toml_has_key(&raw, "model") {
        if !updated.is_empty() {
            updated.push('\n');
        }
        updated.push_str(&format!("model = \"{DEFAULT_MODEL}\"\n"));
        changed = true;
    }

    if !toml_has_key(&raw, "base_url") {
        if !updated.is_empty() {
            updated.push('\n');
        }
        updated.push_str(&format!("base_url = \"{DEFAULT_BASE_URL}\"\n"));
        changed = true;
    }

    if changed {
        std::fs::write(path, updated.as_bytes())
            .with_context(|| format!("write {}", path.display()))?;
    }
    Ok(())
}

fn toml_has_key(content: &str, key: &str) -> bool {
    for line in content.lines() {
        let head = line.split('#').next().unwrap_or("").trim();
        if head.is_empty() || head.starts_with('[') {
            continue;
        }
        if let Some((name, _)) = head.split_once('=') {
            if name.trim() == key {
                return true;
            }
        }
    }
    false
}
