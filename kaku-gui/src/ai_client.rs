//! AI client for Kaku's built-in chat overlay.
//!
//! Reads API config from `~/.config/kaku/assistant.toml` and provides
//! a synchronous streaming chat completion client (OpenAI-compatible API).
//! Supports function/tool calling for agentic workflows.
//!
//! Runs on a plain OS thread (inside overlay), so blocking I/O is fine.

use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use crate::{ai_auth, ai_gemini};

const DEFAULT_MODEL: &str = "gpt-5.4-mini";
const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";

/// Configuration loaded from `assistant.toml`.
#[derive(Clone)]
#[allow(dead_code)]
pub struct AssistantConfig {
    pub api_key: String,
    /// Chat overlay model. Falls back to `model` from assistant.toml when omitted.
    pub chat_model: String,
    /// Optional user-curated model list for the chat overlay. When set, the chat
    /// overlay cycles only through these via Shift+Tab and skips the auto-fetch step.
    pub chat_model_choices: Vec<String>,
    pub base_url: String,
    /// Provider name derived from base_url and auth_type (e.g. "OpenAI", "Copilot").
    pub provider: String,
    /// Auth mechanism: "api_key" (default), "copilot", "codex", or "gemini_key".
    pub auth_type: String,
    /// When false, the `tools` field is omitted from chat requests.
    /// Set `chat_tools_enabled = false` in assistant.toml for providers that do not
    /// support function calling (e.g. some Kimi or local-model variants).
    pub chat_tools_enabled: bool,
    /// Web search provider: "brave", "pipellm", or "tavily". None = disabled.
    pub web_search_provider: Option<String>,
    /// API key for web_search_provider. None = search tool not registered.
    pub web_search_api_key: Option<String>,
    /// Hidden escape hatch: path to a custom fetch script (not in TUI or template).
    /// Script receives the URL as $1 and must print Markdown to stdout.
    pub web_fetch_script: Option<String>,
    /// Optional dedicated model for background memory curation. Falls back to
    /// `chat_model` when unset. Point at a cheaper/faster model to reduce cost.
    pub memory_curator_model: Option<String>,
}

impl AssistantConfig {
    pub fn load() -> Result<Self> {
        let path = assistant_toml_path()?;
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("Cannot read {}", path.display()))?;
        let parsed: toml::Value = raw.parse().context("Invalid assistant.toml")?;

        let auth_type = parsed
            .get("auth_type")
            .and_then(|v| v.as_str())
            .unwrap_or("api_key")
            .to_string();

        // OAuth providers (Copilot, Codex) do not need an api_key in the TOML.
        let api_key_required = matches!(auth_type.as_str(), "api_key" | "gemini_key");

        let api_key = parsed
            .get("api_key")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        if api_key_required && api_key.trim().is_empty() {
            anyhow::bail!(
                "api_key not set in {}. Run `kaku ai` to configure.",
                path.display()
            );
        }

        let model = parsed
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or(DEFAULT_MODEL)
            .to_string();

        // chat_model defaults to `model` to preserve current behavior for users
        // who haven't set an explicit chat model.
        let chat_model = parsed
            .get("chat_model")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| model.clone());

        let chat_model_choices = parsed
            .get("chat_model_choices")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let base_url = parsed
            .get("base_url")
            .and_then(|v| v.as_str())
            .unwrap_or(DEFAULT_BASE_URL)
            .trim_end_matches('/')
            .to_string();

        let provider = detect_provider_with_auth(&base_url, &auth_type).to_string();

        let chat_tools_enabled = parsed
            .get("chat_tools_enabled")
            .and_then(|v| v.as_bool())
            // Gemini tools are supported but off by default until format mapping is verified.
            .unwrap_or(true);

        let web_search_provider = parsed
            .get("web_search_provider")
            .and_then(|v| v.as_str())
            .filter(|s| matches!(*s, "brave" | "pipellm" | "tavily"))
            .map(String::from);

        let web_search_api_key = parsed
            .get("web_search_api_key")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(String::from);

        let web_fetch_script = parsed
            .get("web_fetch_script")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| expand_tilde(s));

        let memory_curator_model = parsed
            .get("memory_curator_model")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(String::from);

        Ok(Self {
            api_key,
            chat_model,
            chat_model_choices,
            base_url,
            provider,
            auth_type,
            chat_tools_enabled,
            web_search_provider,
            web_search_api_key,
            web_fetch_script,
            memory_curator_model,
        })
    }

    /// Returns true when a web_search provider and its API key are both configured.
    pub fn web_search_ready(&self) -> bool {
        self.web_search_provider.is_some() && self.web_search_api_key.is_some()
    }
}

fn expand_tilde(s: &str) -> String {
    if let Some(rest) = s.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return Path::new(&home).join(rest).to_string_lossy().into_owned();
        }
    }
    s.to_string()
}

fn assistant_toml_path() -> Result<PathBuf> {
    let user_config_path = config::user_config_path();
    let config_dir = user_config_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("invalid user config path"))?;
    Ok(config_dir.join("assistant.toml"))
}

// ─── Message types ────────────────────────────────────────────────────────────

/// A single message in API format. Stored as a raw JSON value so it can represent
/// any role (system, user, assistant, tool) including tool_calls and tool results.
#[derive(Clone)]
pub struct ApiMessage(pub serde_json::Value);

impl ApiMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self(serde_json::json!({ "role": "system", "content": content.into() }))
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self(serde_json::json!({ "role": "user", "content": content.into() }))
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self(serde_json::json!({ "role": "assistant", "content": content.into() }))
    }
    /// Assistant turn that requested tool calls (content is null per the OpenAI spec).
    pub fn assistant_tool_calls(tool_calls: serde_json::Value) -> Self {
        Self(serde_json::json!({
            "role": "assistant",
            "content": null,
            "tool_calls": tool_calls
        }))
    }
    /// Tool result message returned after executing a function call.
    /// Includes the tool name so non-OpenAI providers (for example Gemini)
    /// can map responses back to the corresponding function declaration.
    pub fn tool_result(
        tool_call_id: impl Into<String>,
        name: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self(serde_json::json!({
            "role": "tool",
            "tool_call_id": tool_call_id.into(),
            "name": name.into(),
            "content": content.into()
        }))
    }

    /// Approximate serialized byte size of this message. Used for history-budget
    /// accounting in the agent loop; does not need to be exact.
    pub fn byte_len(&self) -> usize {
        serde_json::to_vec(&self.0).map(|v| v.len()).unwrap_or(0)
    }
}

// ─── Tool calling ─────────────────────────────────────────────────────────────

/// A fully assembled tool call returned by the model after streaming is complete.
pub struct ToolCall {
    pub id: String,
    pub name: String,
    /// Complete JSON-encoded arguments string, e.g. `{"path": "~/Downloads"}`.
    pub arguments: String,
}

// ─── Client ───────────────────────────────────────────────────────────────────

/// Synchronous AI client for use inside overlay threads.
/// Clone is cheap: reqwest::blocking::Client is Arc-backed internally.
#[derive(Clone)]
pub struct AiClient {
    config: AssistantConfig,
    client: reqwest::blocking::Client,
}

/// Process-level HTTP client shared across all overlay sessions.
/// TLS stack is initialized once; subsequent `AiClient::new` calls are free.
fn shared_http_client() -> &'static reqwest::blocking::Client {
    static CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::blocking::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(30))
            .timeout(std::time::Duration::from_secs(600))
            .build()
            .unwrap_or_else(|e| {
                log::warn!("Failed to build HTTP client: {e}; falling back to default client");
                reqwest::blocking::Client::new()
            })
    })
}

impl AiClient {
    pub fn new(config: AssistantConfig) -> Self {
        Self {
            config,
            client: shared_http_client().clone(),
        }
    }

    /// Whether this client will include tools in chat requests.
    pub fn tools_enabled(&self) -> bool {
        self.config.chat_tools_enabled
    }

    /// Returns a reference to the loaded assistant configuration.
    pub fn config(&self) -> &AssistantConfig {
        &self.config
    }

    /// Single-shot (non-streaming) completion for short tasks like title generation.
    ///
    /// Internally uses `chat_step` with an empty tools list and accumulates all tokens
    /// into a String. The returned text is trimmed of leading/trailing whitespace.
    pub fn complete_once(&self, model: &str, messages: &[ApiMessage]) -> Result<String> {
        let cancelled = AtomicBool::new(false);
        let mut text = String::new();
        self.chat_step(model, messages, &[], &cancelled, &mut |tok| {
            text.push_str(tok);
        })?;
        Ok(text.trim().to_string())
    }

    /// Fetch available chat models from `{base_url}/models`.
    /// Filters out non-chat models (embeddings, TTS, image, etc.).
    pub fn list_models(&self) -> Result<Vec<String>> {
        // Gemini uses a different models listing endpoint.
        if self.config.auth_type == "gemini_key" {
            return self.list_gemini_models();
        }

        let url = format!("{}/models", self.config.base_url);
        let req = self.client.get(&url);
        let req = self.apply_auth_headers(req)?;
        let resp = req.send().context("GET /models failed")?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            anyhow::bail!("models API {}: {}", status, body);
        }
        let v: serde_json::Value = resp.json().context("parse /models response")?;
        let arr = v
            .get("data")
            .and_then(|d| d.as_array())
            .ok_or_else(|| anyhow::anyhow!("missing `data` array in /models response"))?;
        let mut out: Vec<String> = arr
            .iter()
            .filter_map(|m| m.get("id").and_then(|s| s.as_str()).map(String::from))
            .filter(|id| kaku_ai_utils::is_chat_model_id(id))
            .collect();
        out.sort();
        out.dedup();
        out.truncate(30);
        Ok(out)
    }

    fn list_gemini_models(&self) -> Result<Vec<String>> {
        let api_key = &self.config.api_key;
        let base = self.config.base_url.trim_end_matches('/');
        let url = format!("{base}/v1beta/models");
        let mut req = self.client.get(&url);
        if !api_key.is_empty() {
            req = req.header("x-goog-api-key", api_key);
        }
        let resp = req.send().context("GET Gemini /models failed")?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            anyhow::bail!("Gemini models API {}: {}", status, body);
        }
        let v: serde_json::Value = resp.json().context("parse Gemini /models response")?;
        let arr = match v.get("models").and_then(|m| m.as_array()) {
            Some(a) => a,
            None => return Ok(vec![]),
        };
        let mut out: Vec<String> = arr
            .iter()
            .filter_map(|m| {
                let name = m.get("name")?.as_str()?;
                // name format: "models/gemini-2.5-pro" -> extract the short id
                Some(name.strip_prefix("models/").unwrap_or(name).to_string())
            })
            .filter(|id| {
                let id_lower = id.to_ascii_lowercase();
                id_lower.starts_with("gemini")
            })
            .collect();
        out.sort();
        out.dedup();
        out.truncate(20);
        Ok(out)
    }

    /// Build provider-specific auth headers for the HTTP request builder.
    fn apply_auth_headers(
        &self,
        req: reqwest::blocking::RequestBuilder,
    ) -> Result<reqwest::blocking::RequestBuilder> {
        match self.config.auth_type.as_str() {
            "copilot" => {
                let token = ai_auth::get_copilot_token(&self.client)?;
                Ok(req
                    .header("Authorization", format!("Bearer {token}"))
                    .header("Copilot-Integration-Id", "vscode-chat")
                    .header("Editor-Version", "vscode/1.110.1")
                    .header("Editor-Plugin-Version", "copilot-chat/0.38.2")
                    .header("Openai-Organization", "github-copilot")
                    .header("Openai-Intent", "conversation-panel"))
            }
            "codex" => {
                let token = ai_auth::read_codex_access_token().ok_or_else(|| {
                    anyhow::anyhow!("Codex: not logged in. Run `codex auth login` to authenticate.")
                })?;
                Ok(req.header("Authorization", format!("Bearer {token}")))
            }
            _ => {
                // api_key, gemini_key (api_key field used as Bearer or query param handled elsewhere)
                Ok(req.header("Authorization", format!("Bearer {}", self.config.api_key)))
            }
        }
    }

    /// Single chat step with optional tool support.
    ///
    /// Streams text tokens via `on_token`. If the model responds by requesting
    /// tool calls instead of (or before) text, returns those calls for the
    /// caller to execute and loop. Returns an empty vec when the step is text-only.
    ///
    /// The caller must set `cancelled` to `true` to abort mid-stream.
    pub fn chat_step(
        &self,
        model: &str,
        messages: &[ApiMessage],
        tools: &[serde_json::Value],
        cancelled: &AtomicBool,
        on_token: &mut dyn FnMut(&str),
    ) -> Result<Vec<ToolCall>> {
        // Gemini uses a completely different API format.
        if self.config.auth_type == "gemini_key" {
            return self.chat_step_gemini(model, messages, tools, cancelled, on_token);
        }

        let url = format!("{}/chat/completions", self.config.base_url);

        let mut body = serde_json::json!({
            "model": model,
            "messages": messages.iter().map(|m| m.0.clone()).collect::<Vec<_>>(),
            "stream": true,
        });
        if !tools.is_empty() && self.config.chat_tools_enabled {
            body["tools"] = serde_json::Value::Array(tools.to_vec());
        }

        let req = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Accept", "text/event-stream")
            .header("Cache-Control", "no-cache")
            .header("Accept-Encoding", "identity")
            .json(&body);
        let req = self.apply_auth_headers(req)?;
        let response = req.send().context("HTTP request failed")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            anyhow::bail!("API error {}: {}", status, body);
        }

        let reader = BufReader::new(response);
        // Accumulate tool call fragments by index; each index is one pending call.
        // BTreeMap keeps indices sorted so we process them in order.
        let mut tc_buf: BTreeMap<usize, ToolCallBuf> = BTreeMap::new();
        let mut finish_reason = String::new();

        for line in reader.lines() {
            if cancelled.load(Ordering::Relaxed) {
                break;
            }
            let line = line.context("read SSE line")?;
            let Some(data) = line.strip_prefix("data: ") else {
                continue;
            };
            if data.trim() == "[DONE]" {
                break;
            }
            let chunk = match serde_json::from_str::<serde_json::Value>(data) {
                Ok(v) => v,
                Err(e) => {
                    log::warn!("Failed to parse SSE chunk: {e}");
                    continue;
                }
            };

            let choice = &chunk["choices"][0];

            // Capture finish_reason when present.
            if let Some(fr) = choice["finish_reason"].as_str() {
                if !fr.is_empty() && fr != "null" {
                    finish_reason = fr.to_string();
                }
            }

            let delta = &choice["delta"];

            // Text delta (standard) and reasoning delta (DeepSeek et al.).
            if let Some(reasoning) = delta["reasoning_content"]
                .as_str()
                .or_else(|| choice["reasoning"].as_str())
            {
                if !reasoning.is_empty() {
                    on_token(&format!("<think>\n{}\n</think>\n\n", reasoning));
                }
            }
            if let Some(content) = delta["content"].as_str() {
                on_token(content);
            }

            // Tool call deltas: accumulate arguments by index.
            if let Some(tc_arr) = delta["tool_calls"].as_array() {
                for tc in tc_arr {
                    let idx = tc["index"].as_u64().unwrap_or(0) as usize;
                    let entry = tc_buf.entry(idx).or_default();
                    if let Some(id) = tc["id"].as_str() {
                        entry.id = id.to_string();
                    }
                    if let Some(name) = tc["function"]["name"].as_str() {
                        entry.name = name.to_string();
                    }
                    if let Some(args) = tc["function"]["arguments"].as_str() {
                        entry.arguments.push_str(args);
                    }
                }
            }
        }

        // Build ToolCall results. Some proxies (e.g. vivgrid) never set
        // finish_reason to "tool_calls" even when streaming tool call deltas,
        // so fall back to any accumulated tc_buf entries with a valid name.
        if finish_reason == "tool_calls" || !tc_buf.is_empty() {
            let calls = tc_buf
                .into_values()
                .filter(|b| !b.name.is_empty())
                .map(|b| ToolCall {
                    id: b.id,
                    name: b.name,
                    arguments: b.arguments,
                })
                .collect::<Vec<_>>();
            if calls.is_empty() {
                Ok(vec![])
            } else {
                Ok(calls)
            }
        } else {
            Ok(vec![])
        }
    }

    fn chat_step_gemini(
        &self,
        model: &str,
        messages: &[ApiMessage],
        tools: &[serde_json::Value],
        cancelled: &AtomicBool,
        on_token: &mut dyn FnMut(&str),
    ) -> Result<Vec<ToolCall>> {
        let raw_messages: Vec<serde_json::Value> = messages.iter().map(|m| m.0.clone()).collect();
        let effective_tools: &[serde_json::Value] = if self.config.chat_tools_enabled {
            tools
        } else {
            &[]
        };

        let body = ai_gemini::openai_messages_to_gemini(&raw_messages, effective_tools);
        let url = ai_gemini::gemini_stream_url(&self.config.base_url, model);

        let mut req = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Accept", "text/event-stream")
            .header("Accept-Encoding", "identity");
        if !self.config.api_key.is_empty() {
            req = req.header("x-goog-api-key", &self.config.api_key);
        }
        let response = req
            .json(&body)
            .send()
            .context("Gemini HTTP request failed")?;

        if !response.status().is_success() {
            let status = response.status();
            let body_text = response.text().unwrap_or_default();
            anyhow::bail!("Gemini API error {}: {}", status, body_text);
        }

        ai_gemini::stream_gemini_response(response, cancelled, on_token)
    }
}

// ─── Private helpers ──────────────────────────────────────────────────────────

/// Buffer for accumulating streamed tool call fragments.
#[derive(Default)]
struct ToolCallBuf {
    id: String,
    name: String,
    arguments: String,
}

/// Maps (base_url, auth_type) to a display provider name.
///
/// Mirrors `assistant_config::detect_provider_with_auth` from the kaku crate;
/// kept local to avoid a cross-binary dependency.
fn detect_provider_with_auth(base_url: &str, auth_type: &str) -> &'static str {
    let normalized = base_url.trim().trim_end_matches('/').to_ascii_lowercase();
    match (normalized.as_str(), auth_type) {
        (u, _) if u == "https://api.githubcopilot.com" => "Copilot",
        (u, _) if u == "https://generativelanguage.googleapis.com" => "Gemini",
        (u, "codex") if u == "https://api.openai.com/v1" => "Codex",
        _ => "Custom",
    }
}

// Delegated to kaku-ai-utils crate to avoid cross-binary drift.
