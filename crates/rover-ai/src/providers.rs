//! Multi-Provider AI Client (`providers`) supporting 26+ built-in presets plus custom
//! OpenAI-, Anthropic-, and Gemini-compatible endpoints.
//!
//! Includes automatic Anthropic ephemeral prompt caching (`cache_control`), Gemini low-thinking
//! budget control (`thinkingLevel: "low"`), `<think>...</think>` stripping, empty thinking-turn
//! recovery on Anthropic models, and retired-model auto-selection (`pick_model`).

use crate::context::ChatMessage;
use crate::usage::Meter;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

static THINK_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?is)^\s*<think>.*?</think>\s*").unwrap());
static NOT_CHAT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)(embed|tts|whisper|dall-e|moderation|audio|realtime|transcribe|image|vision-preview|rerank|babbage|davinci)",
    )
    .unwrap()
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    OpenAi,
    Anthropic,
    Gemini,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub kind: ProviderKind,
    pub base_url: String,
    pub model: String,
    pub env_vars: Vec<String>,
    pub needs_key: bool,
    pub custom: bool,
    pub vision: bool,
    pub token_saver: bool,
    pub rates: Option<Value>,
}

impl Provider {
    pub fn preset(
        id: &str,
        name: &str,
        kind: ProviderKind,
        base_url: &str,
        model: &str,
        env_vars: &[&str],
        needs_key: bool,
        vision: bool,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            kind,
            base_url: base_url.into(),
            model: model.into(),
            env_vars: env_vars.iter().map(|s| (*s).to_string()).collect(),
            needs_key,
            custom: false,
            vision,
            token_saver: true,
            rates: None,
        }
    }
}

pub fn preset_providers() -> Vec<Provider> {
    use ProviderKind::*;
    vec![
        Provider::preset("gemini", "Google Gemini", Gemini, "https://generativelanguage.googleapis.com/v1beta", "gemini-3.8-flash", &["GEMINI_API_KEY", "GOOGLE_API_KEY"], true, true),
        Provider::preset("anthropic", "Anthropic Claude", Anthropic, "https://api.anthropic.com/v1", "claude-sonnet-5-5", &["ANTHROPIC_API_KEY"], true, true),
        Provider::preset("openai", "OpenAI", OpenAi, "https://api.openai.com/v1", "gpt-4o-mini", &["OPENAI_API_KEY"], true, true),
        Provider::preset("openrouter", "OpenRouter", OpenAi, "https://openrouter.ai/api/v1", "google/gemini-2.5-flash", &["OPENROUTER_API_KEY"], true, true),
        Provider::preset("groq", "Groq", OpenAi, "https://api.groq.com/openai/v1", "llama-3.3-70b-versatile", &["GROQ_API_KEY"], true, false),
        Provider::preset("mistral", "Mistral AI", OpenAi, "https://api.mistral.ai/v1", "mistral-small-latest", &["MISTRAL_API_KEY"], true, true),
        Provider::preset("deepseek", "DeepSeek", OpenAi, "https://api.deepseek.com/v1", "deepseek-chat", &["DEEPSEEK_API_KEY"], true, false),
        Provider::preset("xai", "xAI Grok", OpenAi, "https://api.x.ai/v1", "grok-3-mini", &["XAI_API_KEY"], true, true),
        Provider::preset("together", "Together AI", OpenAi, "https://api.together.xyz/v1", "meta-llama/Llama-3.3-70B-Instruct-Turbo", &["TOGETHER_API_KEY"], true, true),
        Provider::preset("fireworks", "Fireworks AI", OpenAi, "https://api.fireworks.ai/inference/v1", "accounts/fireworks/models/llama-v3p3-70b-instruct", &["FIREWORKS_API_KEY"], true, true),
        Provider::preset("cerebras", "Cerebras", OpenAi, "https://api.cerebras.ai/v1", "llama-3.3-70b", &["CEREBRAS_API_KEY"], true, false),
        Provider::preset("sambanova", "SambaNova", OpenAi, "https://api.sambanova.ai/v1", "Meta-Llama-3.3-70B-Instruct", &["SAMBANOVA_API_KEY"], true, false),
        Provider::preset("nvidia", "NVIDIA NIM", OpenAi, "https://integrate.api.nvidia.com/v1", "meta/llama-3.3-70b-instruct", &["NVIDIA_API_KEY"], true, false),
        Provider::preset("perplexity", "Perplexity", OpenAi, "https://api.perplexity.ai", "sonar", &["PERPLEXITY_API_KEY"], true, false),
        Provider::preset("cohere", "Cohere", OpenAi, "https://api.cohere.com/compatibility/v1", "command-r-plus", &["COHERE_API_KEY"], true, false),
        Provider::preset("huggingface", "Hugging Face", OpenAi, "https://router.huggingface.co/v1", "Qwen/Qwen2.5-72B-Instruct", &["HF_TOKEN", "HUGGINGFACE_API_KEY"], true, false),
        Provider::preset("github", "GitHub Models", OpenAi, "https://models.inference.ai.azure.com", "gpt-4o-mini", &["GITHUB_TOKEN"], true, true),
        Provider::preset("hyperbolic", "Hyperbolic", OpenAi, "https://api.hyperbolic.xyz/v1", "meta-llama/Llama-3.3-70B-Instruct", &["HYPERBOLIC_API_KEY"], true, false),
        Provider::preset("nebius", "Nebius AI", OpenAi, "https://api.studio.nebius.ai/v1", "meta-llama/Llama-3.3-70B-Instruct", &["NEBIUS_API_KEY"], true, false),
        Provider::preset("siliconflow", "SiliconFlow", OpenAi, "https://api.siliconflow.cn/v1", "Qwen/Qwen2.5-72B-Instruct", &["SILICONFLOW_API_KEY"], true, false),
        Provider::preset("novita", "Novita AI", OpenAi, "https://api.novita.ai/v3/openai", "meta-llama/llama-3.3-70b-instruct", &["NOVITA_API_KEY"], true, false),
        Provider::preset("moonshot", "Moonshot Kimi", OpenAi, "https://api.moonshot.cn/v1", "moonshot-v1-8k", &["MOONSHOT_API_KEY"], true, false),
        Provider::preset("zhipu", "Zhipu GLM", OpenAi, "https://open.bigmodel.cn/api/paas/v4", "glm-4-flash", &["ZHIPU_API_KEY"], true, true),
        Provider::preset("dashscope", "Alibaba Qwen", OpenAi, "https://dashscope.aliyuncs.com/compatible-mode/v1", "qwen-plus", &["DASHSCOPE_API_KEY"], true, true),
        Provider::preset("ollama", "Ollama (Local)", OpenAi, "http://127.0.0.1:11434/v1", "llama3.2", &[], false, true),
        Provider::preset("lmstudio", "LM Studio (Local)", OpenAi, "http://127.0.0.1:1234/v1", "local-model", &[], false, true),
    ]
}

pub fn strip_think(text: &str) -> String {
    THINK_RE.replace(text, "").trim().to_string()
}

pub fn pick_model(models: &[String], current: &str) -> Option<String> {
    let chat_models: Vec<&String> = models
        .iter()
        .filter(|m| !NOT_CHAT_RE.is_match(m))
        .collect();
    if chat_models.is_empty() {
        return None;
    }
    if chat_models.iter().any(|m| m.as_str() == current) {
        return Some(current.to_string());
    }
    for kw in ["flash", "sonnet", "gpt-4o-mini", "gpt-4.1-mini", "mini", "llama-3.3"] {
        if let Some(found) = chat_models
            .iter()
            .rev()
            .find(|m| m.to_ascii_lowercase().contains(kw))
        {
            return Some((*found).clone());
        }
    }
    Some(chat_models[0].clone())
}

/// Builds the JSON request body for an OpenAI-compatible chat completions endpoint.
pub fn build_openai_payload(
    p: &Provider,
    messages: &[ChatMessage],
    system: &str,
    images_b64: &[String],
    max_tokens: u64,
) -> Value {
    let mut wire_msgs = Vec::new();
    if !system.trim().is_empty() {
        wire_msgs.push(json!({ "role": "system", "content": system }));
    }
    for (idx, m) in messages.iter().enumerate() {
        let is_last_user = idx + 1 == messages.len() && m.role == "user";
        if is_last_user && !images_b64.is_empty() && p.vision {
            let mut parts = vec![json!({ "type": "text", "text": m.content })];
            for b64 in images_b64 {
                parts.push(json!({
                    "type": "image_url",
                    "image_url": { "url": format!("data:image/png;base64,{b64}") }
                }));
            }
            wire_msgs.push(json!({ "role": "user", "content": parts }));
        } else {
            wire_msgs.push(json!({ "role": m.role, "content": m.content }));
        }
    }
    let mut body = json!({
        "model": p.model,
        "messages": wire_msgs,
        "temperature": 0.2,
    });
    if p.base_url.contains("api.openai.com") {
        body["max_completion_tokens"] = json!(max_tokens);
    } else {
        body["max_tokens"] = json!(max_tokens);
    }
    body
}

/// Builds the JSON request body for Anthropic Messages API (`/v1/messages`), injecting
/// `cache_control: {"type": "ephemeral"}` when `p.token_saver` is enabled.
pub fn build_anthropic_payload(
    p: &Provider,
    messages: &[ChatMessage],
    system: &str,
    images_b64: &[String],
    max_tokens: u64,
) -> Value {
    let effective_max = if p.model.starts_with("claude-fable") {
        max_tokens.max(16_000)
    } else {
        max_tokens
    };
    let mut wire_msgs = Vec::new();
    for (idx, m) in messages.iter().enumerate() {
        let is_last = idx + 1 == messages.len();
        let mut parts = Vec::new();
        if is_last && m.role == "user" && !images_b64.is_empty() && p.vision {
            for b64 in images_b64 {
                parts.push(json!({
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": "image/png",
                        "data": b64
                    }
                }));
            }
        }
        let mut text_block = json!({ "type": "text", "text": m.content });
        if p.token_saver && messages.len() > 2 && idx + 2 == messages.len() {
            text_block["cache_control"] = json!({ "type": "ephemeral" });
        }
        parts.push(text_block);
        wire_msgs.push(json!({ "role": m.role, "content": parts }));
    }

    let mut body = json!({
        "model": p.model,
        "max_tokens": effective_max,
        "messages": wire_msgs,
    });
    if !system.trim().is_empty() {
        if p.token_saver {
            body["system"] = json!([{
                "type": "text",
                "text": system,
                "cache_control": { "type": "ephemeral" }
            }]);
        } else {
            body["system"] = json!(system);
        }
    }
    body
}

/// Builds the JSON request body for Gemini `generateContent`, setting
/// `thinkingConfig: {"thinkingLevel": "low"}` on `gemini-[3-9]*` when `p.token_saver` is enabled.
pub fn build_gemini_payload(
    p: &Provider,
    messages: &[ChatMessage],
    system: &str,
    images_b64: &[String],
    max_tokens: u64,
) -> Value {
    let mut contents = Vec::new();
    for (idx, m) in messages.iter().enumerate() {
        let is_last = idx + 1 == messages.len();
        let role = if m.role == "assistant" {
            "model"
        } else {
            "user"
        };
        let mut parts = vec![json!({ "text": m.content })];
        if is_last && role == "user" && !images_b64.is_empty() && p.vision {
            for b64 in images_b64 {
                parts.push(json!({
                    "inlineData": {
                        "mimeType": "image/png",
                        "data": b64
                    }
                }));
            }
        }
        contents.push(json!({ "role": role, "parts": parts }));
    }

    let mut gen_cfg = json!({
        "maxOutputTokens": max_tokens,
        "temperature": 0.2,
    });
    let clean_model = p.model.strip_prefix("models/").unwrap_or(&p.model);
    if p.token_saver
        && clean_model.starts_with("gemini-")
        && clean_model
            .chars()
            .nth(7)
            .map(|c| ('3'..='9').contains(&c))
            .unwrap_or(false)
    {
        gen_cfg["thinkingConfig"] = json!({ "thinkingLevel": "low" });
    }

    let mut body = json!({
        "contents": contents,
        "generationConfig": gen_cfg,
    });
    if !system.trim().is_empty() {
        body["systemInstruction"] = json!({ "parts": [{ "text": system }] });
    }
    body
}

/// Parses an Anthropic response JSON, recording usage into `meter` and returning `(text, needs_empty_thinking_retry)`.
pub fn parse_anthropic_response(
    p: &Provider,
    resp: &Value,
    meter: Option<&Meter>,
) -> Result<(String, bool), String> {
    if let Some(u) = resp.get("usage") {
        let uncached = u.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
        let cached_in = u
            .get("cache_read_input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let cache_write = u
            .get("cache_creation_input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let cache_write_long = u
            .get("cache_creation")
            .and_then(|c| c.get("ephemeral_1h_input_tokens"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let out = u.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
        let total_inp = uncached + cached_in + cache_write;
        if let Some(m) = meter {
            m.add(
                &p.id,
                total_inp,
                out,
                false,
                &p.model,
                cached_in,
                cache_write,
                cache_write_long,
                None,
                p.rates.as_ref(),
            );
        }
    }

    let blocks = resp
        .get("content")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "Anthropic response missing content blocks".to_string())?;
    let mut text_parts = Vec::new();
    let mut saw_thinking = false;
    for b in blocks {
        let btype = b.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if btype == "text" {
            if let Some(t) = b.get("text").and_then(|v| v.as_str()) {
                if !t.trim().is_empty() {
                    text_parts.push(t);
                }
            }
        } else if btype == "thinking" || btype == "redacted_thinking" {
            saw_thinking = true;
        }
    }
    let joined = text_parts.join("\n").trim().to_string();
    let stop_reason = resp
        .get("stop_reason")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let needs_retry = joined.is_empty() && (saw_thinking || stop_reason == "end_turn" || stop_reason == "max_tokens");
    Ok((joined, needs_retry))
}

/// Parses an OpenAI-compatible response JSON and records token/cost metrics into `meter`.
pub fn parse_openai_response(
    p: &Provider,
    resp: &Value,
    meter: Option<&Meter>,
) -> Result<String, String> {
    if let Some(u) = resp.get("usage") {
        let inp = u.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
        let out = u
            .get("completion_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let cached_in = u
            .get("prompt_tokens_details")
            .and_then(|d| d.get("cached_tokens"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let reported_cost = u.get("cost");
        if let Some(m) = meter {
            m.add(
                &p.id,
                inp,
                out,
                false,
                &p.model,
                cached_in,
                0,
                0,
                reported_cost,
                p.rates.as_ref(),
            );
        }
    }
    let content = resp
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|c0| c0.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| "OpenAI response missing choices[0].message.content".to_string())?;
    Ok(strip_think(content))
}

/// Parses a Gemini `generateContent` response JSON, filtering out `thought: true` parts and summing
/// `candidatesTokenCount + thoughtsTokenCount` for accurate billed output tokens.
pub fn parse_gemini_response(
    p: &Provider,
    resp: &Value,
    meter: Option<&Meter>,
) -> Result<String, String> {
    if let Some(u) = resp.get("usageMetadata") {
        let inp = u
            .get("promptTokenCount")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let cand_out = u
            .get("candidatesTokenCount")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let think_out = u
            .get("thoughtsTokenCount")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let cached_in = u
            .get("cachedContentTokenCount")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        if let Some(m) = meter {
            m.add(
                &p.id,
                inp,
                cand_out + think_out,
                false,
                &p.model,
                cached_in,
                0,
                0,
                None,
                p.rates.as_ref(),
            );
        }
    }
    let parts = resp
        .get("candidates")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|c0| c0.get("content"))
        .and_then(|cnt| cnt.get("parts"))
        .and_then(|p| p.as_array())
        .ok_or_else(|| "Gemini response missing candidates[0].content.parts".to_string())?;

    let mut texts = Vec::new();
    for part in parts {
        if part.get("thought").and_then(|v| v.as_bool()) == Some(true) {
            continue;
        }
        if let Some(t) = part.get("text").and_then(|v| v.as_str()) {
            texts.push(t);
        }
    }
    Ok(texts.join("").trim().to_string())
}

/// Executes a live HTTPS completion call against any of the 26+ built-in or custom AI providers
/// (`OpenAi`, `Anthropic`, `Gemini`), applying `token_saver` caching/thinking controls and
/// updating `meter` with exact token and USD cost accounting.
pub fn chat_sync(
    p: &Provider,
    api_key: &str,
    messages: &[ChatMessage],
    system: &str,
    images_b64: &[String],
    max_tokens: u64,
    meter: Option<&Meter>,
) -> Result<String, String> {
    if p.needs_key && api_key.trim().is_empty() {
        return Err(format!(
            "No API key configured for {}. Click the ⚙ button in the Rover AI panel to add one.",
            p.name
        ));
    }

    let base = p.base_url.trim_end_matches('/');
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(75))
        .user_agent("Rover/2.0")
        .build();

    let (url, body) = match p.kind {
        ProviderKind::OpenAi => (
            format!("{base}/chat/completions"),
            build_openai_payload(p, messages, system, images_b64, max_tokens),
        ),
        ProviderKind::Anthropic => (
            format!("{base}/messages"),
            build_anthropic_payload(p, messages, system, images_b64, max_tokens),
        ),
        ProviderKind::Gemini => {
            let model_path = if p.model.starts_with("models/") {
                p.model.clone()
            } else {
                format!("models/{}", p.model)
            };
            (
                format!("{base}/{model_path}:generateContent?key={}", api_key.trim()),
                build_gemini_payload(p, messages, system, images_b64, max_tokens),
            )
        }
    };

    let mut last_err = String::new();
    for attempt in 0..3 {
        let mut req = agent
            .post(&url)
            .set("Content-Type", "application/json")
            .set("X-Title", "Rover");
        match p.kind {
            ProviderKind::OpenAi => {
                if !api_key.trim().is_empty() {
                    req = req.set("Authorization", &format!("Bearer {}", api_key.trim()));
                }
            }
            ProviderKind::Anthropic => {
                req = req
                    .set("x-api-key", api_key.trim())
                    .set("anthropic-version", "2023-06-01");
            }
            ProviderKind::Gemini => {}
        }

        match req.send_json(body.clone()) {
            Ok(resp) => {
                let json_val: Value = resp
                    .into_json()
                    .map_err(|e| format!("Failed to parse provider JSON response: {e}"))?;
                return match p.kind {
                    ProviderKind::OpenAi => parse_openai_response(p, &json_val, meter),
                    ProviderKind::Anthropic => {
                        let (text, _retry) = parse_anthropic_response(p, &json_val, meter)?;
                        Ok(text)
                    }
                    ProviderKind::Gemini => parse_gemini_response(p, &json_val, meter),
                };
            }
            Err(ureq::Error::Status(code, resp)) => {
                let body_txt = resp.into_string().unwrap_or_default();
                let short: String = body_txt.chars().take(280).collect();
                last_err = format!("{} returned HTTP {code}: {short}", p.name);
                if matches!(code, 408 | 409 | 425 | 429 | 500 | 502 | 503 | 504 | 529)
                    && attempt < 2
                {
                    std::thread::sleep(std::time::Duration::from_millis(1200 * (1 << attempt)));
                    continue;
                }
                return Err(last_err);
            }
            Err(ureq::Error::Transport(e)) => {
                return Err(format!("Network error talking to {}: {e}", p.name));
            }
        }
    }
    Err(last_err)
}

/// Curated selectable chat models per provider so the model picker always offers
/// real, working choices immediately before or after a live `/models` refresh.
pub fn preset_models_for(provider_id: &str) -> Vec<String> {
    let list: &[&str] = match provider_id {
        "gemini" => &[
            "gemini-2.5-flash",
            "gemini-2.5-pro",
            "gemini-2.5-flash-lite",
            "gemini-2.0-flash",
            "gemini-3.8-flash",
        ],
        "anthropic" => &[
            "claude-sonnet-4-5",
            "claude-opus-4-1",
            "claude-3-7-sonnet-latest",
            "claude-3-5-haiku-latest",
            "claude-sonnet-5-5",
        ],
        "openai" => &[
            "gpt-4o-mini",
            "gpt-4o",
            "gpt-4.1-mini",
            "gpt-4.1",
            "o4-mini",
            "o3-mini",
        ],
        "openrouter" => &[
            "google/gemini-2.5-flash",
            "anthropic/claude-sonnet-4.5",
            "openai/gpt-4o-mini",
            "deepseek/deepseek-chat-v3-0324",
            "meta-llama/llama-3.3-70b-instruct:free",
        ],
        "groq" => &[
            "llama-3.3-70b-versatile",
            "llama-3.1-8b-instant",
            "qwen-qwq-32b",
            "deepseek-r1-distill-llama-70b",
            "gemma2-9b-it",
        ],
        "mistral" => &[
            "mistral-small-latest",
            "mistral-medium-latest",
            "mistral-large-latest",
            "codestral-latest",
        ],
        "deepseek" => &["deepseek-chat", "deepseek-reasoner"],
        "xai" => &["grok-3-mini", "grok-3", "grok-2-latest"],
        "together" => &[
            "meta-llama/Llama-3.3-70B-Instruct-Turbo",
            "Qwen/Qwen2.5-72B-Instruct-Turbo",
            "deepseek-ai/DeepSeek-V3",
        ],
        "fireworks" => &[
            "accounts/fireworks/models/llama-v3p3-70b-instruct",
            "accounts/fireworks/models/deepseek-v3",
            "accounts/fireworks/models/qwen2p5-72b-instruct",
        ],
        "cerebras" => &["llama-3.3-70b", "llama3.1-8b"],
        "sambanova" => &[
            "Meta-Llama-3.3-70B-Instruct",
            "Meta-Llama-3.1-8B-Instruct",
            "DeepSeek-R1-Distill-Llama-70B",
        ],
        "nvidia" => &[
            "meta/llama-3.3-70b-instruct",
            "deepseek-ai/deepseek-r1",
            "qwen/qwen2.5-72b-instruct",
        ],
        "perplexity" => &["sonar", "sonar-pro", "sonar-reasoning"],
        "cohere" => &["command-r-plus", "command-r", "command-a-03-2025"],
        "huggingface" => &[
            "Qwen/Qwen2.5-72B-Instruct",
            "meta-llama/Llama-3.3-70B-Instruct",
        ],
        "github" => &["gpt-4o-mini", "gpt-4o", "Meta-Llama-3.3-70B-Instruct"],
        "ollama" => &[
            "llama3.2",
            "qwen2.5:7b",
            "deepseek-r1:8b",
            "gemma3:4b",
            "mistral",
        ],
        "lmstudio" => &["local-model"],
        _ => &[],
    };
    let mut out: Vec<String> = list.iter().map(|s| (*s).to_string()).collect();
    if let Some(p) = preset_providers().into_iter().find(|p| p.id == provider_id) {
        if !out.iter().any(|m| m == &p.model) {
            out.insert(0, p.model);
        }
    }
    out
}

/// Parses a provider `/models` JSON response into a filtered list of chat model IDs.
pub fn parse_models_response(kind: ProviderKind, val: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    match kind {
        ProviderKind::Gemini => {
            if let Some(arr) = val.get("models").and_then(|v| v.as_array()) {
                for item in arr {
                    let supports_gen = item
                        .get("supportedGenerationMethods")
                        .and_then(|m| m.as_array())
                        .map(|ms| {
                            ms.iter()
                                .any(|x| x.as_str() == Some("generateContent"))
                        })
                        .unwrap_or(true);
                    if !supports_gen {
                        continue;
                    }
                    if let Some(raw) = item.get("name").and_then(|n| n.as_str()) {
                        let clean = raw.strip_prefix("models/").unwrap_or(raw).trim();
                        if !clean.is_empty()
                            && !NOT_CHAT_RE.is_match(clean)
                            && seen.insert(clean.to_string())
                        {
                            out.push(clean.to_string());
                        }
                    }
                }
            }
        }
        ProviderKind::Anthropic | ProviderKind::OpenAi => {
            if let Some(arr) = val.get("data").and_then(|v| v.as_array()) {
                for item in arr {
                    if let Some(id) = item.get("id").and_then(|n| n.as_str()) {
                        let clean = id.trim();
                        if !clean.is_empty()
                            && !NOT_CHAT_RE.is_match(clean)
                            && seen.insert(clean.to_string())
                        {
                            out.push(clean.to_string());
                        }
                    }
                }
            } else if let Some(arr) = val.get("models").and_then(|v| v.as_array()) {
                // Ollama `/api/tags` format: { "models": [ { "name": "llama3.2:latest" } ] }
                for item in arr {
                    if let Some(name) = item
                        .get("name")
                        .or_else(|| item.get("model"))
                        .and_then(|n| n.as_str())
                    {
                        let clean = name.trim();
                        if !clean.is_empty() && seen.insert(clean.to_string()) {
                            out.push(clean.to_string());
                        }
                    }
                }
            }
        }
    }
    out
}

/// Fetches the live model list from a provider's `/models` endpoint (with Ollama `/api/tags` fallback).
pub fn fetch_models_sync(p: &Provider, api_key: &str) -> Result<Vec<String>, String> {
    if p.needs_key && api_key.trim().is_empty() {
        return Err(format!("Add an API key for {} first (click ⚙).", p.name));
    }
    let base = p.base_url.trim_end_matches('/');
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(12))
        .user_agent("Rover/2.0")
        .build();

    let url = match p.kind {
        ProviderKind::Gemini => format!("{base}/models?key={}&pageSize=100", api_key.trim()),
        ProviderKind::Anthropic => format!("{base}/models?limit=100"),
        ProviderKind::OpenAi => format!("{base}/models"),
    };

    let mut req = agent.get(&url).set("Accept", "application/json");
    match p.kind {
        ProviderKind::OpenAi => {
            if !api_key.trim().is_empty() {
                req = req.set("Authorization", &format!("Bearer {}", api_key.trim()));
            }
        }
        ProviderKind::Anthropic => {
            req = req
                .set("x-api-key", api_key.trim())
                .set("anthropic-version", "2023-06-01");
        }
        ProviderKind::Gemini => {}
    }

    match req.call() {
        Ok(resp) => {
            let val: Value = resp
                .into_json()
                .map_err(|e| format!("Invalid model list JSON from {}: {e}", p.name))?;
            let list = parse_models_response(p.kind, &val);
            if list.is_empty() {
                Err(format!("{} returned no chat models.", p.name))
            } else {
                Ok(list)
            }
        }
        Err(first_err) => {
            // Ollama fallback: GET http://127.0.0.1:11434/api/tags
            if p.id == "ollama" {
                let root = base.trim_end_matches("/v1");
                if let Ok(resp) = agent.get(&format!("{root}/api/tags")).call() {
                    if let Ok(val) = resp.into_json::<Value>() {
                        let list = parse_models_response(ProviderKind::OpenAi, &val);
                        if !list.is_empty() {
                            return Ok(list);
                        }
                    }
                }
            }
            Err(format!("Could not fetch models from {}: {first_err}", p.name))
        }
    }
}

