use anyhow::{anyhow, bail, Context};
use clap::Parser;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Parser, Clone, Default)]
pub struct InitCommand {
    /// Refresh shell integration without interactive prompts
    #[arg(long)]
    pub update_only: bool,
}

impl InitCommand {
    pub fn run(&self) -> anyhow::Result<()> {
        imp::run(self.update_only)
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use anyhow::bail;

    pub fn run(_update_only: bool) -> anyhow::Result<()> {
        bail!("`kaku init` is currently supported on macOS only")
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    pub fn run(update_only: bool) -> anyhow::Result<()> {
        ensure_user_config().context("ensure user config exists")?;
        ensure_opencode_config().context("ensure opencode config exists")?;

        install_kaku_wrapper().context("install kaku wrapper")?;

        let script = resolve_setup_script()
            .ok_or_else(|| anyhow!("failed to locate setup_zsh.sh for Kaku initialization"))?;

        let mut cmd = Command::new("/bin/bash");
        cmd.arg(&script).env("KAKU_INIT_INTERNAL", "1");
        if update_only {
            cmd.arg("--update-only");
        }
        let status = cmd
            .status()
            .with_context(|| format!("run {}", script.display()))?;

        if status.success() {
            return Ok(());
        }

        bail!("kaku init failed with status {}", status);
    }

    fn install_kaku_wrapper() -> anyhow::Result<()> {
        let wrapper_path = wrapper_path();
        let wrapper_dir = wrapper_path
            .parent()
            .ok_or_else(|| anyhow!("invalid wrapper path"))?;
        config::create_user_owned_dirs(wrapper_dir).context("create wrapper directory")?;

        if fs::symlink_metadata(&wrapper_path)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
        {
            fs::remove_file(&wrapper_path).with_context(|| {
                format!("remove legacy symlink wrapper {}", wrapper_path.display())
            })?;
        }

        let preferred_bin = resolve_preferred_kaku_bin()
            .unwrap_or_else(|| PathBuf::from("/Applications/Kaku.app/Contents/MacOS/kaku"));
        let preferred_bin = escape_for_double_quotes(&preferred_bin.display().to_string());

        let script = format!(
            r#"#!/bin/bash
set -euo pipefail

if [[ -n "${{KAKU_BIN:-}}" && -x "${{KAKU_BIN}}" ]]; then
	exec "${{KAKU_BIN}}" "$@"
fi

for candidate in \
	"{preferred_bin}" \
	"/Applications/Kaku.app/Contents/MacOS/kaku" \
	"$HOME/Applications/Kaku.app/Contents/MacOS/kaku"; do
	if [[ -n "$candidate" && -x "$candidate" ]]; then
		exec "$candidate" "$@"
	fi
done

echo "kaku: Kaku.app not found. Expected /Applications/Kaku.app." >&2
exit 127
"#
        );

        let mut file = fs::File::create(&wrapper_path)
            .with_context(|| format!("create wrapper {}", wrapper_path.display()))?;
        file.write_all(script.as_bytes())
            .with_context(|| format!("write wrapper {}", wrapper_path.display()))?;
        fs::set_permissions(&wrapper_path, fs::Permissions::from_mode(0o755))
            .with_context(|| format!("chmod wrapper {}", wrapper_path.display()))?;
        Ok(())
    }

    fn wrapper_path() -> PathBuf {
        config::HOME_DIR
            .join(".config")
            .join("kaku")
            .join("zsh")
            .join("bin")
            .join("kaku")
    }

    fn resolve_preferred_kaku_bin() -> Option<PathBuf> {
        if let Some(path) = std::env::var_os("KAKU_BIN") {
            let path = PathBuf::from(path);
            if path.exists() {
                return Some(path);
            }
        }

        if let Ok(exe) = std::env::current_exe() {
            if exe
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.eq_ignore_ascii_case("kaku"))
                .unwrap_or(false)
                && exe.exists()
            {
                return Some(exe);
            }
        }

        for candidate in [
            PathBuf::from("/Applications/Kaku.app/Contents/MacOS/kaku"),
            config::HOME_DIR
                .join("Applications")
                .join("Kaku.app")
                .join("Contents")
                .join("MacOS")
                .join("kaku"),
        ] {
            if candidate.exists() {
                return Some(candidate);
            }
        }

        None
    }

    fn escape_for_double_quotes(value: &str) -> String {
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('$', "\\$")
            .replace('`', "\\`")
    }

    fn resolve_setup_script() -> Option<PathBuf> {
        let mut candidates = Vec::new();

        if let Ok(cwd) = std::env::current_dir() {
            candidates.push(
                cwd.join("assets")
                    .join("shell-integration")
                    .join("setup_zsh.sh"),
            );
        }

        if let Ok(exe) = std::env::current_exe() {
            if let Some(contents_dir) = exe.parent().and_then(|p| p.parent()) {
                candidates.push(contents_dir.join("Resources").join("setup_zsh.sh"));
            }
        }

        candidates.push(PathBuf::from(
            "/Applications/Kaku.app/Contents/Resources/setup_zsh.sh",
        ));
        candidates.push(
            config::HOME_DIR
                .join("Applications")
                .join("Kaku.app")
                .join("Contents")
                .join("Resources")
                .join("setup_zsh.sh"),
        );

        candidates.into_iter().find(|p| p.exists())
    }

    fn ensure_user_config() -> anyhow::Result<()> {
        let config_path = resolve_user_config_path();
        if config_path.exists() {
            return Ok(());
        }

        let parent = config_path
            .parent()
            .ok_or_else(|| anyhow!("invalid config path: {}", config_path.display()))?;
        config::create_user_owned_dirs(parent).context("create config directory")?;

        std::fs::write(&config_path, minimal_user_config_template())
            .context("write user config file")?;
        Ok(())
    }

    fn ensure_opencode_config() -> anyhow::Result<()> {
        let opencode_dir = config::HOME_DIR.join(".config").join("opencode");
        let opencode_config = opencode_dir.join("opencode.json");
        let themes_dir = opencode_dir.join("themes");

        if opencode_config.exists() {
            return Ok(());
        }

        config::create_user_owned_dirs(&opencode_dir)
            .context("create opencode config directory")?;
        config::create_user_owned_dirs(&themes_dir).context("create opencode themes directory")?;

        let theme_content = r##"{
  "$schema": "https://opencode.ai/theme.json",
  "defs": {
    "bg": "#15141b",
    "panel": "#15141b",
    "element": "#1f1d28",
    "text": "#edecee",
    "muted": "#6b6b6b",
    "primary": "#a277ff",
    "secondary": "#61ffca",
    "accent": "#ffca85",
    "error": "#ff6767",
    "warning": "#ffca85",
    "success": "#61ffca",
    "info": "#a277ff",
    "border": "#15141b",
    "border_active": "#29263c",
    "border_subtle": "#15141b"
  },
  "theme": {
    "primary": "primary",
    "secondary": "secondary",
    "accent": "accent",
    "error": "error",
    "warning": "warning",
    "success": "success",
    "info": "info",
    "text": "text",
    "textMuted": "muted",
    "background": "bg",
    "backgroundPanel": "panel",
    "backgroundElement": "element",
    "border": "border",
    "borderActive": "border_active",
    "borderSubtle": "border_subtle",
    "diffAdded": "success",
    "diffRemoved": "error",
    "diffContext": "muted",
    "diffHunkHeader": "primary",
    "diffHighlightAdded": "success",
    "diffHighlightRemoved": "error",
    "diffAddedBg": "#1b2a24",
    "diffRemovedBg": "#2a1b20",
    "diffContextBg": "bg",
    "diffLineNumber": "muted",
    "diffAddedLineNumberBg": "#1b2a24",
    "diffRemovedLineNumberBg": "#2a1b20",
    "markdownText": "text",
    "markdownHeading": "primary",
    "markdownLink": "info",
    "markdownLinkText": "primary",
    "markdownCode": "accent",
    "markdownBlockQuote": "muted",
    "markdownEmph": "accent",
    "markdownStrong": "secondary",
    "markdownHorizontalRule": "muted",
    "markdownListItem": "primary",
    "markdownListEnumeration": "accent",
    "markdownImage": "info",
    "markdownImageText": "primary",
    "markdownCodeBlock": "text",
    "syntaxComment": "muted",
    "syntaxKeyword": "primary",
    "syntaxFunction": "secondary",
    "syntaxVariable": "text",
    "syntaxString": "success",
    "syntaxNumber": "accent",
    "syntaxType": "info",
    "syntaxOperator": "primary",
    "syntaxPunctuation": "text"
  }
}
"##;

        let theme_file = themes_dir.join("wezterm-match.json");
        std::fs::write(&theme_file, theme_content).context("write opencode theme file")?;

        let config_content = r#"{
  "theme": "wezterm-match"
}
"#;

        std::fs::write(&opencode_config, config_content).context("write opencode config file")?;
        Ok(())
    }

    fn resolve_user_config_path() -> PathBuf {
        config::CONFIG_DIRS
            .first()
            .cloned()
            .unwrap_or_else(|| config::HOME_DIR.join(".config").join("kaku"))
            .join("kaku.lua")
    }

    fn minimal_user_config_template() -> &'static str {
        r#"local wezterm = require 'wezterm'

local function resolve_bundled_config()
  local resource_dir = wezterm.executable_dir:gsub('MacOS/?$', 'Resources')
  local bundled = resource_dir .. '/kaku.lua'
  local f = io.open(bundled, 'r')
  if f then
    f:close()
    return bundled
  end

  local dev_bundled = wezterm.executable_dir .. '/../../assets/macos/Kaku.app/Contents/Resources/kaku.lua'
  f = io.open(dev_bundled, 'r')
  if f then
    f:close()
    return dev_bundled
  end

  local app_bundled = '/Applications/Kaku.app/Contents/Resources/kaku.lua'
  f = io.open(app_bundled, 'r')
  if f then
    f:close()
    return app_bundled
  end

  local home = os.getenv('HOME') or ''
  local home_bundled = home .. '/Applications/Kaku.app/Contents/Resources/kaku.lua'
  f = io.open(home_bundled, 'r')
  if f then
    f:close()
    return home_bundled
  end

  return nil
end

local config = {}
local bundled = resolve_bundled_config()

if bundled then
  local ok, loaded = pcall(dofile, bundled)
  if ok and type(loaded) == 'table' then
    config = loaded
  else
    wezterm.log_error('Kaku: failed to load bundled defaults from ' .. bundled)
  end
else
  wezterm.log_error('Kaku: bundled defaults not found')
end

return config
"#
    }
}
