//! OAuth token management for AI providers that use OAuth instead of API keys.
//!
//! Copilot: exchanges a GitHub OAuth token (set by the TUI device-code flow)
//! for a short-lived Copilot API token, caching it in copilot_auth.json.
//!
//! Codex: reads the access token written by the Codex CLI into ~/.codex/auth.json.

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const COPILOT_TOKEN_URL: &str = "https://api.github.com/copilot_internal/v2/token";

// ─── Token file paths ─────────────────────────────────────────────────────────

pub fn copilot_auth_file_path() -> Option<PathBuf> {
    let user_config_path = config::user_config_path();
    let config_dir = user_config_path.parent()?;
    Some(config_dir.join("copilot_auth.json"))
}

fn codex_auth_file_path() -> PathBuf {
    config::HOME_DIR.join(".codex").join("auth.json")
}

// ─── Copilot auth ─────────────────────────────────────────────────────────────

#[derive(serde::Serialize, serde::Deserialize, Default, Clone)]
struct CopilotAuthFile {
    pub github_token: String,
    #[serde(default)]
    pub copilot_token: String,
    /// Unix seconds when the cached Copilot token expires.
    #[serde(default)]
    pub copilot_expires_at: u64,
}

fn load_copilot_auth() -> Option<CopilotAuthFile> {
    let path = copilot_auth_file_path()?;
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| log::debug!("copilot auth read failed: {e}"))
        .ok()?;
    serde_json::from_str(&raw)
        .map_err(|e| log::debug!("copilot auth parse failed: {e}"))
        .ok()
}

fn save_copilot_auth(auth: &CopilotAuthFile) -> Result<()> {
    let path = copilot_auth_file_path()
        .ok_or_else(|| anyhow::anyhow!("cannot determine copilot auth path"))?;
    let json = serde_json::to_vec_pretty(auth).context("serialize copilot auth")?;
    std::fs::write(&path, &json).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Returns a valid Copilot API token, exchanging/refreshing via the GitHub token
/// stored in copilot_auth.json when the cached token is expired or missing.
pub fn get_copilot_token(client: &reqwest::blocking::Client) -> Result<String> {
    let mut auth = load_copilot_auth().ok_or_else(|| {
        anyhow::anyhow!(
            "Copilot: not logged in. Open `kaku ai` and select Copilot, then press Enter on \
             the GitHub Auth field to authenticate."
        )
    })?;

    if auth.github_token.trim().is_empty() {
        anyhow::bail!("Copilot: GitHub token missing. Open `kaku ai` and authenticate via GitHub.");
    }

    // Refresh 60 seconds before expiry so tokens don't expire mid-request.
    let needs_refresh =
        auth.copilot_token.is_empty() || now_unix_secs() + 60 >= auth.copilot_expires_at;

    if needs_refresh {
        let resp = client
            .get(COPILOT_TOKEN_URL)
            .header(
                "Authorization",
                format!("Bearer {}", auth.github_token.trim()),
            )
            .header("Accept", "application/json")
            .header("User-Agent", "kaku/1.0")
            .header("Editor-Version", "vscode/1.110.1")
            .header("Editor-Plugin-Version", "copilot-chat/0.38.2")
            .header("Copilot-Integration-Id", "vscode-chat")
            .send()
            .context("fetch Copilot token from GitHub")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            anyhow::bail!("Copilot token refresh failed ({}): {}", status, body);
        }

        let data: serde_json::Value = resp.json().context("parse Copilot token response")?;
        let token = data["token"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing `token` field in Copilot token response"))?
            .to_string();

        // expires_at may be ISO 8601 (string) or Unix seconds (number).
        let expires_at = if let Some(secs) = data["expires_at"].as_u64() {
            secs
        } else if let Some(s) = data["expires_at"].as_str() {
            parse_iso8601_to_unix(s).unwrap_or_else(|| now_unix_secs() + 1500)
        } else {
            now_unix_secs() + 1500 // fallback: 25 minutes
        };

        auth.copilot_token = token;
        auth.copilot_expires_at = expires_at;

        if let Err(e) = save_copilot_auth(&auth) {
            log::warn!("Failed to persist refreshed Copilot token: {e}");
        }
    }

    Ok(auth.copilot_token.clone())
}

/// Returns true when copilot_auth.json exists and has a GitHub token.
#[allow(dead_code)]
pub fn copilot_is_authenticated() -> bool {
    load_copilot_auth().is_some_and(|auth| !auth.github_token.trim().is_empty())
}

/// Very minimal ISO 8601 UTC parser: "2024-12-01T10:00:00+00:00" -> Unix seconds.
/// Only handles the format returned by the GitHub Copilot token API.
fn parse_iso8601_to_unix(s: &str) -> Option<u64> {
    // Use chrono if available; otherwise fall back to a simple parse.
    // Format: YYYY-MM-DDTHH:MM:SS+00:00 or YYYY-MM-DDTHH:MM:SSZ
    let s = s.trim();
    // Strip timezone: take up to the first +/Z after the time.
    let without_tz = s
        .find('+')
        .or_else(|| s.rfind('Z'))
        .map(|idx| &s[..idx])
        .unwrap_or(s);

    let parts: Vec<&str> = without_tz.splitn(2, 'T').collect();
    if parts.len() != 2 {
        return None;
    }
    let date_parts: Vec<u32> = parts[0].split('-').filter_map(|p| p.parse().ok()).collect();
    let time_parts: Vec<u32> = parts[1].split(':').filter_map(|p| p.parse().ok()).collect();
    if date_parts.len() < 3 || time_parts.len() < 3 {
        return None;
    }

    // Days since Unix epoch: very rough approximation (ignores leap years precisely).
    let year = date_parts[0] as u64;
    let month = date_parts[1] as u64;
    let day = date_parts[2] as u64;
    // Gregorian day count (Zeller-adjacent, good enough for timestamps).
    let a = (14 - month) / 12;
    let y = year + 4800 - a;
    let m = month + 12 * a - 3;
    let jdn = day + (153 * m + 2) / 5 + 365 * y + y / 4 - y / 100 + y / 400 - 32045;
    // Julian Day Number for Unix epoch (1970-01-01) = 2440588
    let unix_days = jdn.saturating_sub(2440588);

    let h = time_parts[0] as u64;
    let min = time_parts[1] as u64;
    let sec = time_parts[2] as u64;

    Some(unix_days * 86400 + h * 3600 + min * 60 + sec)
}

// ─── Codex auth ───────────────────────────────────────────────────────────────

/// Reads the Codex CLI access token from ~/.codex/auth.json.
///
/// Codex stores its OAuth tokens here after `codex auth login`. Kaku reads the
/// token to authenticate against the OpenAI API on the user's behalf.
pub fn read_codex_access_token() -> Option<String> {
    let raw = std::fs::read_to_string(codex_auth_file_path())
        .map_err(|e| log::debug!("codex auth read failed: {e}"))
        .ok()?;
    let v: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|e| log::debug!("codex auth parse failed: {e}"))
        .ok()?;

    // Codex auth.json has two observed shapes: {"tokens":{"access_token":"..."}}
    // and {"access_token":"..."}.
    v.get("tokens")
        .and_then(|t| t.get("access_token"))
        .or_else(|| v.get("access_token"))
        .and_then(|t| t.as_str())
        .filter(|t| !t.is_empty())
        .map(String::from)
}

/// Returns true when the Codex CLI auth file exists and has a token.
#[allow(dead_code)]
pub fn codex_is_authenticated() -> bool {
    read_codex_access_token().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_iso8601_zulu() {
        // 1970-01-01T00:00:00Z -> 0
        assert_eq!(parse_iso8601_to_unix("1970-01-01T00:00:00Z"), Some(0));
    }

    #[test]
    fn parse_iso8601_offset() {
        // 2024-01-01T00:00:00+00:00 -> some positive value
        let ts = parse_iso8601_to_unix("2024-01-01T00:00:00+00:00");
        assert!(ts.is_some());
        assert!(ts.unwrap() > 1_700_000_000);
    }
}
