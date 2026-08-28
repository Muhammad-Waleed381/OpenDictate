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
        PolishProvider::Groq => polish_with_groq(trimmed, config),
        PolishProvider::LocalSlm => {
            // Local SLM placeholder / fallback until ONNX model is provisioned
            log::info!("Local SLM polish requested; falling back to clean text");
            Ok(trimmed.to_string())
        }
    }
}

/// Calls Groq's high-speed LPU API to polish text.
fn polish_with_groq(text: &str, config: &PolishConfig) -> Result<String> {
    let api_key = config
        .groq_api_key
        .as_ref()
        .filter(|k| !k.trim().is_empty())
        .ok_or_else(|| {
            CoreError::Transcription("Groq API key is missing or empty".to_string())
        })?;

    let model = config
        .groq_model
        .as_deref()
        .unwrap_or(DEFAULT_GROQ_MODEL);

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
                "content": text
            }
        ]
    });

    // Generous global timeout: Groq completions with up to 1024 output tokens
    // regularly exceed 4 s under load, and every timeout means the polish is
    // silently skipped (callers fall back to raw text).
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(15)))
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

    #[derive(Deserialize)]
    struct GroqChoiceMessage {
        content: String,
    }

    #[derive(Deserialize)]
    struct GroqChoice {
        message: GroqChoiceMessage,
    }

    #[derive(Deserialize)]
    struct GroqResponse {
        choices: Option<Vec<GroqChoice>>,
    }

    let parsed: GroqResponse = resp.into_body().read_json().map_err(|e| {
        log::warn!("failed to parse Groq response: {e}");
        CoreError::Transcription(format!("invalid Groq response: {e}"))
    })?;

    if let Some(choices) = parsed.choices {
        if let Some(first) = choices.into_iter().next() {
            let output = first.message.content.trim().to_string();
            if !output.is_empty() {
                return Ok(output);
            }
        }
    }

    Ok(text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

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
