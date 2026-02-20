use anyhow::{anyhow, bail, Context};
use clap::Parser;
use std::path::{Path, PathBuf};
use std::process::Command;

const DEFAULT_MODEL: &str = "gpt-5-mini";
const DEFAULT_BASE_URL: &str = "https://api.vivgrid.com/v1";

#[derive(Debug, Parser, Clone, Default)]
pub struct AutoCommand {}

impl AutoCommand {
    pub fn run(&self) -> anyhow::Result<()> {
        let path = ensure_auto_toml_exists()?;
        ensure_auto_toml_base_url(&path)?;
        open_config(&path)?;
        println!("Opened config: {}", path.display());
        Ok(())
    }
}

fn ensure_auto_toml_exists() -> anyhow::Result<PathBuf> {
    let path = auto_toml_path()?;
    if path.exists() {
        return Ok(path);
    }

    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("invalid auto.toml path: {}", path.display()))?;
    config::create_user_owned_dirs(parent).context("create config directory")?;

    std::fs::write(&path, default_auto_toml_template())
        .with_context(|| format!("write {}", path.display()))?;
    Ok(path)
}

fn auto_toml_path() -> anyhow::Result<PathBuf> {
    let user_config_path = config::user_config_path();
    let config_dir = user_config_path
        .parent()
        .ok_or_else(|| anyhow!("invalid user config path: {}", user_config_path.display()))?;
    Ok(config_dir.join("auto.toml"))
}

fn default_auto_toml_template() -> String {
    format!(
        "# Kaku Auto AI configuration\n\
# enabled: true enables AI error analysis; false disables all AI calls.\n\
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

fn ensure_auto_toml_base_url(path: &Path) -> anyhow::Result<()> {
    let raw = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    if toml_has_key(&raw, "base_url") {
        return Ok(());
    }

    let mut updated = raw.trim_end().to_string();
    if !updated.is_empty() {
        updated.push('\n');
    }
    updated.push_str(&format!("base_url = \"{DEFAULT_BASE_URL}\"\n"));

    std::fs::write(path, updated.as_bytes())
        .with_context(|| format!("write {}", path.display()))?;
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

fn open_config(config_path: &Path) -> anyhow::Result<()> {
    if open_with_editor(config_path)? {
        return Ok(());
    }

    let status = Command::new("/usr/bin/open")
        .arg(config_path)
        .status()
        .context("open config file with default app")?;
    if status.success() {
        return Ok(());
    }
    bail!("failed to open config file: {}", config_path.display());
}

fn open_with_editor(config_path: &Path) -> anyhow::Result<bool> {
    let Some(editor) = std::env::var_os("EDITOR") else {
        return Ok(false);
    };

    let editor = editor.to_string_lossy().trim().to_string();
    if editor.is_empty() {
        return Ok(false);
    }

    let parts = shell_words::split(&editor)
        .with_context(|| format!("failed to parse EDITOR value `{}`", editor))?;
    if parts.is_empty() {
        return Ok(false);
    }

    let status = Command::new(&parts[0])
        .args(parts.iter().skip(1))
        .arg(config_path)
        .status()
        .with_context(|| format!("launch editor `{}`", parts[0]))?;

    Ok(status.success())
}
