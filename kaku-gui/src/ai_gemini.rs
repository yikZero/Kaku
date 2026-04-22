//! Gemini API format conversion for Kaku's AI client.
//!
//! Converts between OpenAI-format messages/tools used internally and the
//! Google Generative Language API format used by Gemini.
//!
//! Endpoint pattern: POST {base_url}/v1beta/models/{model}:streamGenerateContent?key={api_key}

use anyhow::{Context, Result};
use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::ai_client::ToolCall;

// ─── Request conversion ───────────────────────────────────────────────────────

/// Convert a slice of OpenAI-format messages to a Gemini `generateContent` body.
///
/// System messages become `systemInstruction`; user/assistant messages become
/// `contents` with `role: "user"` or `role: "model"`. Tool calls and results
/// are mapped to Gemini `functionCall` / `functionResponse` parts.
pub fn openai_messages_to_gemini(
    messages: &[serde_json::Value],
    tools: &[serde_json::Value],
) -> serde_json::Value {
    let mut system_parts: Vec<serde_json::Value> = Vec::new();
    let mut contents: Vec<serde_json::Value> = Vec::new();

    for msg in messages {
        let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("user");
        match role {
            "system" => {
                let text = msg
                    .get("content")
                    .and_then(|c| c.as_str())
                    .unwrap_or_default();
                system_parts.push(serde_json::json!({"text": text}));
            }
            "assistant" => {
                let mut parts: Vec<serde_json::Value> = Vec::new();
                if let Some(content) = msg.get("content").and_then(|c| c.as_str()) {
                    if !content.is_empty() {
                        parts.push(serde_json::json!({"text": content}));
                    }
                }
                // Tool calls from the assistant.
                if let Some(tool_calls) = msg.get("tool_calls").and_then(|tc| tc.as_array()) {
                    for tc in tool_calls {
                        let name = tc
                            .get("function")
                            .and_then(|f| f.get("name"))
                            .and_then(|n| n.as_str())
                            .unwrap_or("unknown");
                        let raw_args = tc
                            .get("function")
                            .and_then(|f| f.get("arguments"))
                            .and_then(|a| a.as_str())
                            .unwrap_or("{}");
                        let args: serde_json::Value =
                            serde_json::from_str(raw_args).unwrap_or(serde_json::json!({}));
                        parts.push(
                            serde_json::json!({"functionCall": {"name": name, "args": args}}),
                        );
                    }
                }
                if !parts.is_empty() {
                    contents.push(serde_json::json!({"role": "model", "parts": parts}));
                }
            }
            "tool" => {
                // Tool results come as role="tool" in OpenAI format.
                let name = msg
                    .get("name")
                    .and_then(|n| n.as_str())
                    .or_else(|| msg.get("tool_call_id").and_then(|n| n.as_str()))
                    .unwrap_or("unknown");
                let content = msg.get("content").and_then(|c| c.as_str()).unwrap_or("{}");
                // Gemini expects tool results as a user turn.
                let response: serde_json::Value = serde_json::from_str(content)
                    .unwrap_or_else(|_| serde_json::json!({"content": content}));
                contents.push(serde_json::json!({
                    "role": "user",
                    "parts": [{"functionResponse": {"name": name, "response": response}}]
                }));
            }
            _ => {
                // "user" role.
                let text = msg
                    .get("content")
                    .and_then(|c| c.as_str())
                    .unwrap_or_default();
                contents.push(serde_json::json!({"role": "user", "parts": [{"text": text}]}));
            }
        }
    }

    let mut body = serde_json::json!({
        "contents": contents,
        "generationConfig": {
            "maxOutputTokens": 8192,
        }
    });

    if !system_parts.is_empty() {
        body["systemInstruction"] = serde_json::json!({"parts": system_parts});
    }

    if !tools.is_empty() {
        let decls: Vec<serde_json::Value> = tools
            .iter()
            .filter_map(|t| {
                let func = t.get("function")?;
                let name = func.get("name")?.as_str()?;
                let mut decl = serde_json::json!({"name": name});
                if let Some(desc) = func.get("description").and_then(|d| d.as_str()) {
                    decl["description"] = serde_json::Value::String(desc.to_string());
                }
                if let Some(params) = func.get("parameters") {
                    decl["parameters"] = params.clone();
                }
                Some(decl)
            })
            .collect();
        if !decls.is_empty() {
            body["tools"] = serde_json::json!([{"functionDeclarations": decls}]);
        }
    }

    body
}

/// Build the Gemini streaming endpoint URL.
pub fn gemini_stream_url(base_url: &str, model: &str, api_key: &str) -> String {
    let base = base_url.trim().trim_end_matches('/');
    if api_key.is_empty() {
        format!("{base}/v1beta/models/{model}:streamGenerateContent")
    } else {
        format!("{base}/v1beta/models/{model}:streamGenerateContent?key={api_key}")
    }
}

// ─── Response streaming ───────────────────────────────────────────────────────

/// Stream a Gemini response, forwarding text tokens via `on_token` and
/// returning any function calls at the end.
pub fn stream_gemini_response(
    response: reqwest::blocking::Response,
    cancelled: &AtomicBool,
    on_token: &mut dyn FnMut(&str),
) -> Result<Vec<ToolCall>> {
    let reader = BufReader::new(response);
    // Keep function calls in arrival order. Avoid keying by name so repeated
    // same-name calls in one turn are preserved.
    let mut function_calls: Vec<FunctionCallBuf> = Vec::new();
    let mut finish_reason = String::new();

    for line in reader.lines() {
        if cancelled.load(Ordering::Relaxed) {
            break;
        }
        let line = line.context("read Gemini SSE line")?;
        let data = match line.strip_prefix("data: ") {
            Some(d) => d,
            None => {
                if line.trim() == "data: [DONE]" || line.trim() == "[DONE]" {
                    break;
                }
                continue;
            }
        };
        if data.trim() == "[DONE]" {
            break;
        }

        let chunk = match serde_json::from_str::<serde_json::Value>(data) {
            Ok(v) => v,
            Err(e) => {
                log::warn!("Failed to parse Gemini SSE chunk: {e}");
                continue;
            }
        };

        let candidate = &chunk["candidates"][0];
        if let Some(fr) = candidate["finishReason"].as_str() {
            if !fr.is_empty() && fr != "null" && fr != "STOP" {
                finish_reason = fr.to_string();
            }
        }

        let parts = match candidate["content"]["parts"].as_array() {
            Some(p) => p,
            None => continue,
        };

        for part in parts {
            if let Some(text) = part["text"].as_str() {
                if !text.is_empty() {
                    on_token(text);
                }
            }
            if let Some(fc) = part.get("functionCall") {
                let name = fc["name"].as_str().unwrap_or("unknown").to_string();
                let arguments = fc
                    .get("args")
                    .map(|args| serde_json::to_string(args).unwrap_or_else(|_| "{}".to_string()))
                    .unwrap_or_else(|| "{}".to_string());

                // Some providers may emit duplicate chunks; suppress exact
                // adjacent duplicates while preserving distinct same-name calls.
                let is_adjacent_duplicate = function_calls
                    .last()
                    .is_some_and(|last| last.name == name && last.arguments == arguments);
                if !is_adjacent_duplicate {
                    function_calls.push(FunctionCallBuf { name, arguments });
                }
            }
        }
    }

    if finish_reason == "FUNCTION_CALL" || !function_calls.is_empty() {
        let calls = function_calls
            .into_iter()
            .enumerate()
            .map(|(i, buf)| ToolCall {
                id: format!("gemini_fc_{i}"),
                name: buf.name,
                arguments: buf.arguments,
            })
            .collect();
        Ok(calls)
    } else {
        Ok(vec![])
    }
}

struct FunctionCallBuf {
    name: String,
    arguments: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_message_becomes_system_instruction() {
        let messages = vec![
            serde_json::json!({"role": "system", "content": "Be helpful"}),
            serde_json::json!({"role": "user", "content": "Hello"}),
        ];
        let body = openai_messages_to_gemini(&messages, &[]);
        assert!(body.get("systemInstruction").is_some());
        let contents = body["contents"].as_array().unwrap();
        assert_eq!(contents.len(), 1);
        assert_eq!(contents[0]["role"], "user");
    }

    #[test]
    fn assistant_becomes_model_role() {
        let messages = vec![
            serde_json::json!({"role": "user", "content": "Hi"}),
            serde_json::json!({"role": "assistant", "content": "Hello!"}),
        ];
        let body = openai_messages_to_gemini(&messages, &[]);
        let contents = body["contents"].as_array().unwrap();
        assert_eq!(contents[1]["role"], "model");
    }

    #[test]
    fn tools_become_function_declarations() {
        let tools = vec![serde_json::json!({
            "type": "function",
            "function": {
                "name": "search",
                "description": "Search the web",
                "parameters": {"type": "object", "properties": {"query": {"type": "string"}}}
            }
        })];
        let body = openai_messages_to_gemini(&[], &tools);
        let decls = &body["tools"][0]["functionDeclarations"];
        assert_eq!(decls[0]["name"], "search");
    }

    #[test]
    fn gemini_stream_url_with_key() {
        let url = gemini_stream_url(
            "https://generativelanguage.googleapis.com",
            "gemini-2.5-flash",
            "AIzaKey",
        );
        assert!(url.contains("streamGenerateContent?key=AIzaKey"));
        assert!(url.contains("gemini-2.5-flash"));
    }

    #[test]
    fn gemini_stream_url_without_key() {
        let url = gemini_stream_url(
            "https://generativelanguage.googleapis.com",
            "gemini-2.5-pro",
            "",
        );
        assert!(!url.contains("key="));
    }
}
