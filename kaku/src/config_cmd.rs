use anyhow::{bail, Context};
use clap::Parser;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Parser, Clone, Default)]
pub struct ConfigCommand {
    /// Ensure ~/.config/kaku/kaku.lua exists, but do not open it.
    #[arg(long)]
    ensure_only: bool,
}

impl ConfigCommand {
    pub fn run(&self) -> anyhow::Result<()> {
        let config_path = config::ensure_user_config_exists()?;
        if self.ensure_only {
            println!("Ensured config: {}", config_path.display());
            return Ok(());
        }

        open_config(&config_path)?;
        println!("Opened config: {}", config_path.display());
        Ok(())
    }
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
