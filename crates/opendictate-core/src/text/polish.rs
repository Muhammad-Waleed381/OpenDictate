use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::{CoreError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PolishProvider {
    #[default]
    Off,
    Groq,
    LocalSlm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PolishMode {
    #[default]
    Clean,
    Bullets,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PolishConfig {
    pub provider: PolishProvider,
    pub mode: PolishMode,
    pub groq_api_key: Option<String>,
    pub groq_model: Option<String>,
}

const GROQ_API_URL: &str = "https://api.groq.com/openai/v1/chat/completions";
const DEFAULT_GROQ_MODEL: &str = "llama-3.1-8b-instant";

const SYSTEM_PROMPT_CLEAN: &str = "You are an ultra-fast voice dictation polisher. \
Clean up verbal filler words (um, uh, like, you know, ah), false starts, stutters, and fix punctuation/capitalization. \
Preserve the speaker's exact meaning and phrasing as closely as possible. \
Output ONLY the clean polished text without quotes, explanations, preambles, or conversational replies.";

const SYSTEM_PROMPT_BULLETS: &str = "You are an executive voice dictation assistant. \
Transform the following spoken stream of consciousness into crisp, concise markdown bullet points. \
Output ONLY the markdown bullet list, without quotes, explanations, or introductory text.";

/// Applies AI Voice Polish to transcribed text according to the given configuration.
pub fn polish_text(text: &str, config: &PolishConfig) -> Result<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() || config.provider == PolishProvider::Off {
        return Ok(text.to_string());
    }

    match config.provider {
        PolishProvider::Off => Ok(text.to_string()),
        // The request payload is trimmed, but the fallback keeps the original
        // text: a failed or incomplete polish must be a no-op on the complete
        // transcript, including its leading/trailing whitespace.
        PolishProvider::Groq => polish_with_groq(trimmed, text, config),
        PolishProvider::LocalSlm => Err(CoreError::Transcription(
            "Local AI polish is not available yet".into(),
        )),
    }
}

/// Calls Groq's high-speed LPU API to polish text.
fn polish_with_groq(payload: &str, original: &str, config: &PolishConfig) -> Result<String> {
    let api_key = config
        .groq_api_key
        .as_ref()
        .filter(|k| !k.trim().is_empty())
        .ok_or_else(|| CoreError::Transcription("Groq API key is missing or empty".to_string()))?;

    let model = config.groq_model.as_deref().unwrap_or(DEFAULT_GROQ_MODEL);

    let system_prompt = match config.mode {
        PolishMode::Clean => SYSTEM_PROMPT_CLEAN,
        PolishMode::Bullets => SYSTEM_PROMPT_BULLETS,
    };

    let body = json!({
        "model": model,
        "temperature": 0.2,
        "max_tokens": 1024,
        "messages": [
            {
                "role": "system",
                "content": system_prompt
            },
            {
                "role": "user",
                "content": payload
            }
        ]
    });

    // Generous global timeout: Groq completions with up to 1024 output tokens
    // regularly exceed 4 s under load, and every timeout means the polish is
    // silently skipped (callers fall back to raw text).
    // `http_status_as_error(false)` keeps non-2xx responses as responses so
    // the provider's own error body can be surfaced to the user.
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(15)))
        .http_status_as_error(false)
        .build()
        .new_agent();

    let resp = agent
        .post(GROQ_API_URL)
        .header("Authorization", &format!("Bearer {}", api_key.trim()))
        .header("Content-Type", "application/json")
        .send_json(&body)
        .map_err(|e| {
            log::warn!("Groq API request failed: {e}");
            CoreError::Transcription(format!("Groq API error: {e}"))
        })?;

    let status = resp.status();
    let raw = resp.into_body().read_to_string().map_err(|e| {
        log::warn!("failed to read Groq response: {e}");
        CoreError::Transcription(format!("invalid Groq response: {e}"))
    })?;
    if !status.is_success() {
        // Forward the provider's message (invalid key, rate limit, ...) so
        // the user sees something actionable instead of a bare status code.
        let detail = raw.trim();
        log::warn!("Groq API returned {status}: {detail}");
        return Err(CoreError::Transcription(if detail.is_empty() {
            format!("Groq API error: http status {status}")
        } else {
            format!("Groq API error: http status {status}: {detail}")
        }));
    }

    let parsed: GroqResponse = serde_json::from_str(&raw).map_err(|e| {
        log::warn!("failed to parse Groq response: {e}");
        CoreError::Transcription(format!("invalid Groq response: {e}"))
    })?;
    completed_polish_output(parsed, original)
}

#[derive(Deserialize)]
struct GroqChoiceMessage {
    content: String,
}

#[derive(Deserialize)]
struct GroqChoice {
    message: GroqChoiceMessage,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct GroqResponse {
    choices: Option<Vec<GroqChoice>>,
}

/// Finish reasons that mean the provider stopped normally and the response is
/// complete. `length` and `content_filter` mean it was cut short.
const COMPLETE_FINISH_REASONS: [&str; 3] = ["stop", "eos", "stop_sequence"];

fn completed_polish_output(parsed: GroqResponse, text: &str) -> Result<String> {
    if let Some(choices) = parsed.choices {
        if let Some(first) = choices.into_iter().next() {
            let output = first.message.content.trim().to_string();
            let complete = first
                .finish_reason
                .as_deref()
                .is_some_and(|reason| COMPLETE_FINISH_REASONS.contains(&reason));
            if complete && !output.is_empty() {
                return Ok(output);
            }
            if let Some(reason) = first.finish_reason.as_deref() {
                if !COMPLETE_FINISH_REASONS.contains(&reason) {
                    log::warn!("discarding incomplete Groq polish (finish_reason={reason})");
                }
            } else {
                log::warn!("discarding Groq polish without a completion marker");
            }
        }
    }

    Ok(text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incomplete_polish_preserves_complete_original() {
        for reason in [Some("length"), Some("content_filter"), None] {
            let response = serde_json::from_value(json!({"choices":[{
                "message":{"content":"Only the beginning"}, "finish_reason":reason
            }]}))
            .unwrap();
            assert_eq!(
                completed_polish_output(response, "Complete original sentence.").unwrap(),
                "Complete original sentence."
            );
        }
        let response = serde_json::from_value(json!({"choices":[{
            "message":{"content":"Complete polished sentence."}, "finish_reason":"stop"
        }]}))
        .unwrap();
        assert_eq!(
            completed_polish_output(response, "Original").unwrap(),
            "Complete polished sentence."
        );
    }

    #[test]
    fn alternative_completion_markers_are_accepted() {
        for reason in ["stop", "eos", "stop_sequence"] {
            let response = serde_json::from_value(json!({"choices":[{
                "message":{"content":"Complete polished sentence."}, "finish_reason":reason
            }]}))
            .unwrap();
            assert_eq!(
                completed_polish_output(response, "Original").unwrap(),
                "Complete polished sentence.",
                "finish_reason {reason} marks a complete response"
            );
        }
    }

    #[test]
    fn failed_polish_keeps_the_untrimmed_original() {
        // The provider is off, so the text must come back byte for byte.
        let config = PolishConfig {
            provider: PolishProvider::Off,
            ..Default::default()
        };
        assert_eq!(
            polish_text("  padded transcript  ", &config).unwrap(),
            "  padded transcript  "
        );
    }

    #[test]
    fn unavailable_local_polish_reports_unsupported() {
        let config = PolishConfig {
            provider: PolishProvider::LocalSlm,
            ..Default::default()
        };
        assert!(polish_text("original", &config).is_err());
    }

    #[test]
    fn off_provider_returns_original_text() {
        let config = PolishConfig {
            provider: PolishProvider::Off,
            mode: PolishMode::Clean,
            groq_api_key: None,
            groq_model: None,
        };
        assert_eq!(
            polish_text("um hello world like you know", &config).unwrap(),
            "um hello world like you know"
        );
    }
}
