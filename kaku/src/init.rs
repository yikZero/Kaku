use anyhow::{anyhow, bail, Context};
use clap::Parser;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
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
    use crate::utils::{is_jsonc_path, parse_json_or_jsonc, write_atomic};
    use std::os::unix::fs::PermissionsExt;

    pub fn run(update_only: bool) -> anyhow::Result<()> {
        ensure_user_config().context("ensure user config exists")?;
        if !update_only {
            maybe_setup_opencode_config().context("opencode config setup")?;
        }

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
            if is_executable_file(&path) {
                return Some(path);
            }
        }

        if let Ok(exe) = std::env::current_exe() {
            if exe
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.eq_ignore_ascii_case("kaku"))
                .unwrap_or(false)
                && is_executable_file(&exe)
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
            if is_executable_file(&candidate) {
                return Some(candidate);
            }
        }

        None
    }

    fn is_executable_file(path: &Path) -> bool {
        fs::metadata(path)
            .map(|meta| meta.is_file() && (meta.permissions().mode() & 0o111 != 0))
            .unwrap_or(false)
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
        config::ensure_user_config_exists().context("ensure user config exists")?;
        Ok(())
    }

    fn prompt_yes_no(question: &str) -> bool {
        use std::io::{BufRead, Write as _};
        print!("{} [Y/n] ", question);
        if let Err(err) = std::io::stdout().flush() {
            eprintln!("warning: failed to flush stdout before prompt: {err}");
        }
        let stdin = std::io::stdin();
        let mut line = String::new();
        if stdin.lock().read_line(&mut line).is_err() {
            return true;
        }
        let answer = line.trim();
        if answer.is_empty() {
            return true;
        }
        !matches!(answer, "n" | "N" | "no" | "No" | "NO")
    }

    fn maybe_setup_opencode_config() -> anyhow::Result<()> {
        let opencode_dir = config::HOME_DIR.join(".config").join("opencode");
        let opencode_json = opencode_dir.join("opencode.json");
        let opencode_jsonc = opencode_dir.join("opencode.jsonc");
        let themes_dir = opencode_dir.join("themes");

        let has_json = opencode_json.exists();
        let has_jsonc = opencode_jsonc.exists();

        let opencode_config = if has_jsonc {
            Some(opencode_jsonc.clone())
        } else if has_json {
            Some(opencode_json.clone())
        } else {
            None
        };

        if has_json && has_jsonc {
            println!(
                "Both OpenCode config files exist; using {}",
                opencode_jsonc.display()
            );
        }

        if opencode_config.is_some() {
            if !prompt_yes_no("OpenCode config already exists. Overwrite with Kaku theme?") {
                return Ok(());
            }
        } else if !prompt_yes_no("Set up OpenCode with Kaku-matching theme?") {
            return Ok(());
        }

        config::create_user_owned_dirs(&opencode_dir)
            .context("create opencode config directory")?;
        config::create_user_owned_dirs(&themes_dir).context("create opencode themes directory")?;

        let theme_content = crate::ai_config::OPENCODE_THEME_JSON;

        let theme_file = themes_dir.join("wezterm-match.json");
        write_atomic(&theme_file, theme_content.as_bytes()).context("write opencode theme file")?;

        let target_config = opencode_config.unwrap_or(opencode_json);

        let config_content = if target_config.exists() {
            let existing =
                std::fs::read_to_string(&target_config).context("read opencode config file")?;
            let mut json: serde_json::Value =
                parse_json_or_jsonc(&existing).with_context(|| {
                    format!("parse opencode config file: {}", target_config.display())
                })?;
            if let Some(obj) = json.as_object_mut() {
                obj.insert(
                    "theme".to_string(),
                    serde_json::Value::String("wezterm-match".to_string()),
                );
            }
            serde_json::to_string_pretty(&json).unwrap_or_else(|_| existing)
        } else {
            r#"{
  "theme": "wezterm-match"
}"#
            .to_string()
        };

        if is_jsonc_path(&target_config) {
            println!(
                "Note: {} comments will be removed when Kaku rewrites this file.",
                target_config.display()
            );
        }

        write_atomic(&target_config, config_content.as_bytes())
            .context("write opencode config file")?;
        println!("OpenCode theme configured successfully.");
        Ok(())
    }
}
