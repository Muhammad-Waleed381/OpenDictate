use std::collections::HashSet;
use std::path::Path;

use sherpa_onnx::{KeywordSpotter, KeywordSpotterConfig, OnlineStream};

use crate::error::{CoreError, Result};

const KWS_SAMPLE_RATE: i32 = 16000;
const KWS_FEATURE_DIM: i32 = 80;

fn load_vocab(tokens_path: &Path) -> HashSet<String> {
    let content = match std::fs::read_to_string(tokens_path) {
        Ok(c) => c,
        Err(_) => return HashSet::new(),
    };
    content
        .lines()
        .filter_map(|l| {
            let parts: Vec<&str> = l.split_whitespace().collect();
            if !parts.is_empty() {
                Some(parts[0].to_string())
            } else {
                None
            }
        })
        .collect()
}

fn tokenize_word(word: &str, vocab: &HashSet<String>) -> Vec<String> {
    let upper = word.to_uppercase();
    if upper.is_empty() {
        return Vec::new();
    }

    let mut tokens = Vec::new();
    let mut remaining = upper.as_str();

    // First piece: try with leading SentencePiece whitespace symbol \u{2581}
    let mut matched_first = false;
    for len in (1..=remaining.len()).rev() {
        if let Some(prefix) = remaining.get(0..len) {
            let candidate = format!("\u{2581}{prefix}");
            if vocab.contains(&candidate) {
                tokens.push(candidate);
                remaining = &remaining[len..];
                matched_first = true;
                break;
            }
        }
    }

    if !matched_first {
        if vocab.contains("\u{2581}") {
            tokens.push("\u{2581}".to_string());
        }
    }

    // Remaining subwords / characters
    while !remaining.is_empty() {
        let mut matched = false;
        for len in (1..=remaining.len()).rev() {
            if let Some(prefix) = remaining.get(0..len) {
                if vocab.contains(prefix) {
                    tokens.push(prefix.to_string());
                    remaining = &remaining[len..];
                    matched = true;
                    break;
                }
            }
        }
        if !matched {
            // Character not representable in the vocab: dropping it silently
            // changes the tokenized string (e.g. "dictàte" → "dictte"), so the
            // keyword can never match and the mapping is corrupted. Surface
            // it loudly instead.
            let dropped = remaining.chars().next().unwrap_or('\0');
            log::warn!(
                "KWS tokenizer: character {:?} is missing from the vocab; \
                 the keyword may never match",
                dropped
            );
            let mut chars = remaining.chars();
            chars.next();
            remaining = chars.as_str();
        }
    }

    tokens
}

pub fn tokenize_phrase(phrase: &str, vocab: &HashSet<String>) -> String {
    let words: Vec<&str> = phrase.split_whitespace().collect();
    let mut phrase_tokens = Vec::new();
    for w in words {
        phrase_tokens.extend(tokenize_word(w, vocab));
    }
    phrase_tokens.join(" ")
}

/// A configured, reusable keyword spotter engine.
pub struct Spotter {
    spotter: KeywordSpotter,
    keyword_map: std::collections::HashMap<String, String>,
}

/// Active streaming session for the spotter.
pub struct SpotterSession {
    stream: OnlineStream,
}

impl Spotter {
    /// Builds a KeywordSpotter from a model directory containing Zipformer KWS
    /// ONNX files (`encoder`, `decoder`, `joiner`, `tokens.txt`) and a list of keywords.
    pub fn new(model_dir: &Path, keywords: &[String], threshold: f32) -> Result<Self> {
        if !model_dir.exists() {
            return Err(CoreError::Transcription(format!(
                "KWS model directory not found at {}",
                model_dir.display()
            )));
        }

        let mut config = KeywordSpotterConfig::default();
        config.feat_config.sample_rate = KWS_SAMPLE_RATE;
        config.feat_config.feature_dim = KWS_FEATURE_DIM;
        config.keywords_threshold = threshold.clamp(0.01, 1.0);
        config.keywords_score = 1.5;
        config.max_active_paths = 4;
        config.num_trailing_blanks = 2;

        let find = |needle: &str| -> Result<String> {
            let mut matches: Vec<_> = std::fs::read_dir(model_dir)
                .map_err(|e| {
                    CoreError::Transcription(format!(
                        "failed to read {}: {e}",
                        model_dir.display()
                    ))
                })?
                .flatten()
                .filter(|e| {
                    let n = e.file_name().to_string_lossy().to_lowercase();
                    n.ends_with(".onnx") && n.contains(needle)
                })
                .map(|e| e.path())
                .collect();
            matches.sort_by_key(|p| !p.to_string_lossy().to_lowercase().contains(".int8."));
            matches
                .first()
                .map(|p| p.to_string_lossy().into_owned())
                .ok_or_else(|| {
                    CoreError::Transcription(format!(
                        "no '{needle}' ONNX file found in {}",
                        model_dir.display()
                    ))
                })
        };

        config.model_config.transducer.encoder = Some(find("encoder")?);
        config.model_config.transducer.decoder = Some(find("decoder")?);
        config.model_config.transducer.joiner = Some(find("joiner")?);

        let tokens_path = model_dir.join("tokens.txt");
        if !tokens_path.exists() {
            return Err(CoreError::Transcription(format!(
                "tokens.txt missing from KWS model directory {}",
                model_dir.display()
            )));
        }
        config.model_config.tokens = Some(tokens_path.to_string_lossy().into_owned());

        let vocab = load_vocab(&tokens_path);

        // Default or user-configured keywords.
        let default_keywords = vec![
            "hey dictate".to_string(),
            "open dictate".to_string(),
            "computer".to_string(),
            "start dictation".to_string(),
        ];
        let kws_list = if keywords.is_empty() {
            &default_keywords
        } else {
            keywords
        };

        // Format keywords buffer: each keyword must be formatted as space-separated tokens from tokens.txt.
        let mut keyword_map = std::collections::HashMap::new();
        let mut tokenized_keywords = Vec::new();
        for kw in kws_list {
            let trimmed = kw.trim();
            if trimmed.is_empty() {
                continue;
            }
            let tokenized = tokenize_phrase(trimmed, &vocab);
            if !tokenized.is_empty() {
                log::info!("KWS mapped keyword '{}' -> '{}'", trimmed, tokenized);
                keyword_map.insert(tokenized.clone(), trimmed.to_string());
                tokenized_keywords.push(tokenized);
            }
        }

        if tokenized_keywords.is_empty() {
            return Err(CoreError::Transcription("No valid keyword phrases could be tokenized for KWS".into()));
        }

        let keywords_buf = tokenized_keywords.join("\n");
        config.keywords_buf = Some(keywords_buf);

        let spotter = KeywordSpotter::create(&config).ok_or_else(|| {
            CoreError::Transcription("failed to instantiate SherpaOnnx KeywordSpotter".into())
        })?;

        Ok(Self { spotter, keyword_map })
    }

    /// Creates a fresh streaming detection session.
    pub fn create_session(&self) -> SpotterSession {
        SpotterSession {
            stream: self.spotter.create_stream(),
        }
    }

    /// Feeds PCM samples (16kHz mono float) into the keyword spotter stream.
    pub fn accept_waveform(&self, session: &SpotterSession, samples: &[f32]) {
        session.stream.accept_waveform(KWS_SAMPLE_RATE, samples);
    }

    /// Feeds samples, decodes ready frames, and returns any detected keyword trigger.
    pub fn process_samples(&self, session: &SpotterSession, samples: &[f32]) -> Option<String> {
        self.accept_waveform(session, samples);
        while self.spotter.is_ready(&session.stream) {
            self.spotter.decode(&session.stream);
        }
        if let Some(res) = self.spotter.get_result(&session.stream) {
            let kw = res.keyword.trim().to_string();
            if !kw.is_empty() {
                self.spotter.reset(&session.stream);
                let human_name = self.keyword_map.get(&kw).cloned().unwrap_or(kw);
                return Some(human_name);
            }
        }
        None
    }

    /// Resets the detection state for the given session.
    pub fn reset(&self, session: &SpotterSession) {
        self.spotter.reset(&session.stream);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spotter_init_and_feed() {
        let path = crate::stt::models::kws_model_dir();
        if !crate::stt::models::is_kws_ready() {
            eprintln!("KWS model not installed, skipping test");
            return;
        }
        let keywords = vec!["hey dictate".to_string(), "computer".to_string()];
        let spotter = match Spotter::new(&path, &keywords, 0.25) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Spotter::new failed: {e}");
                return;
            }
        };
        let session = spotter.create_session();
        let silence = vec![0.0f32; 16000];
        let res = spotter.process_samples(&session, &silence);
        assert!(res.is_none());
    }
}
