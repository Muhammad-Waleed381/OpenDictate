//! Only supported native decoder/model/tokenizer combinations receive hotwords.
//! Sherpa 1.13.4 exits the process for unsupported overloads or modeling units.
use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub(super) struct HotwordTokenizer {
    pub path: PathBuf,
    vocabulary: HashSet<String>,
    uppercase: bool,
}

impl HotwordTokenizer {
    pub fn load(dir: &Path) -> Option<Self> {
        let tokens_path = dir.join("tokens.txt");
        if std::fs::metadata(&tokens_path).ok()?.len() > 16 * 1024 * 1024 {
            return None;
        }
        let tokens = std::fs::read_to_string(tokens_path).ok()?;
        let mut vocabulary = HashSet::new();
        let mut ids = HashSet::new();
        let mut ordered = Vec::new();
        for line in tokens.lines().filter(|line| !line.trim().is_empty()) {
            let mut parts = line.split_whitespace();
            let symbol = parts.next()?;
            let id = parts.next()?.parse::<u32>().ok()?;
            if parts.next().is_some()
                || symbol.contains('\0')
                || !ids.insert(id)
                || !vocabulary.insert(symbol.to_string())
            {
                return None;
            }
            ordered.push(symbol.to_string());
        }
        if !vocabulary.contains("<blk>")
            || !vocabulary.contains("<unk>")
            || !vocabulary.iter().any(|s| s.starts_with('▁'))
        {
            return None;
        }
        let mut lowercase = 0;
        let mut uppercase = 0;
        for token in vocabulary.iter().filter(|token| !token.starts_with('<')) {
            lowercase += token.chars().filter(|ch| ch.is_lowercase()).count();
            uppercase += token.chars().filter(|ch| ch.is_uppercase()).count();
        }

        // Preserve supplied SentencePiece scores when present. Older archives
        // contain only tokens.txt; sherpa's own examples derive an equal-score
        // vocabulary for native longest-piece encoding in that case.
        let source_vocab = dir.join("bpe.vocab");
        let content = if source_vocab.exists() {
            let text = std::fs::read_to_string(source_vocab).ok()?;
            let mut seen = HashSet::new();
            for line in text.lines().filter(|line| !line.trim().is_empty()) {
                let (symbol, score) = line.split_once('\t')?;
                if !vocabulary.contains(symbol)
                    || !score.parse::<f32>().ok()?.is_finite()
                    || !seen.insert(symbol.to_string())
                {
                    return None;
                }
            }
            // Preflight validation must use exactly the native tokenizer's vocabulary.
            vocabulary.retain(|symbol| seen.contains(symbol));
            if !vocabulary.iter().any(|s| s.starts_with('▁')) {
                return None;
            }
            text
        } else {
            ordered
                .into_iter()
                .map(|symbol| format!("{symbol}\t-1.0\n"))
                .collect()
        };

        // Each recognizer owns a private snapshot. Model removal/replacement
        // cannot invalidate the tokenizer during recognizer construction.
        static NEXT_VOCAB: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "opendictate-bpe-{}-{}.vocab",
            std::process::id(),
            NEXT_VOCAB.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .ok()?;
        if file.write_all(content.as_bytes()).is_err() {
            let _ = std::fs::remove_file(&path);
            return None;
        }
        Some(Self {
            path,
            vocabulary,
            uppercase: uppercase > lowercase,
        })
    }

    fn can_encode(&self, word: &str) -> bool {
        let text = format!("▁{word}");
        let mut reachable = vec![false; text.len() + 1];
        reachable[0] = true;
        for (start, _) in text.char_indices() {
            if !reachable[start] {
                continue;
            }
            for (relative, ch) in text[start..].char_indices() {
                let end = start + relative + ch.len_utf8();
                if self.vocabulary.contains(&text[start..end]) {
                    reachable[end] = true;
                }
            }
        }
        reachable[text.len()]
    }

    pub fn prepare(&self, phrases: &str) -> Option<String> {
        let phrases: Vec<_> = phrases
            .lines()
            .filter_map(|phrase| {
                // Colons are native score syntax, '/' is a phrase delimiter, NUL
                // cannot cross the C boundary. Never reinterpret those as data.
                if phrase
                    .chars()
                    .any(|ch| ch.is_control() || matches!(ch, ':' | '/'))
                {
                    return None;
                }
                let phrase = if self.uppercase {
                    phrase.to_uppercase()
                } else {
                    phrase.to_lowercase()
                };
                let words: Vec<_> = phrase.split_whitespace().collect();
                if words.is_empty()
                    || words
                        .iter()
                        .any(|word| word.len() > 256 || !self.can_encode(word))
                {
                    return None;
                }
                Some(words.join(" "))
            })
            .collect();
        (!phrases.is_empty()).then(|| phrases.join("\n"))
    }
}

impl Drop for HotwordTokenizer {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

pub(super) fn online_supports_hotwords(dir: &Path, is_ctc: bool, hint: Option<&str>) -> bool {
    // Both selectable NeMo streaming families inherit a fatal hotword overload.
    // The catalog's Zipformer caption model has a supported beam decoder.
    !is_ctc
        && dir.file_name().and_then(|name| name.to_str()) == Some(super::models::CAPTION_MODEL_ID)
        && matches!(hint, None | Some("zipformer"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stt::models;

    #[test]
    fn unsupported_online_families_never_take_hotword_overload() {
        assert!(!online_supports_hotwords(
            Path::new(models::FASTCONFORMER_STREAMING_80MS_MODEL_ID),
            true,
            Some("nemo_transducer")
        ));
        assert!(!online_supports_hotwords(
            Path::new(models::PARAKEET_STREAMING_MODEL_ID),
            false,
            Some("nemo_transducer")
        ));
        assert!(!online_supports_hotwords(
            Path::new("unknown-transducer"),
            false,
            None
        ));
        assert!(online_supports_hotwords(
            Path::new(models::CAPTION_MODEL_ID),
            false,
            None
        ));
        assert!(!online_supports_hotwords(
            Path::new(models::CAPTION_MODEL_ID),
            true,
            None
        ));
    }

    #[test]
    fn native_bpe_assets_encode_complete_phrases_without_control_syntax() {
        let dir = std::env::temp_dir().join(format!("opendictate-hotwords-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("tokens.txt"),
            "<blk> 0\n<unk> 1\n▁ 2\n▁OPEN 3\nDIC 4\nTATE 5\n▁C 6\n+ 7\n# 8\n",
        )
        .unwrap();
        let tokenizer = HotwordTokenizer::load(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        let tokenizer = tokenizer.expect("valid SentencePiece tokens should enable native BPE");
        assert_eq!(
            tokenizer.prepare("Open Dictate\nC++\nC#"),
            Some("OPEN DICTATE\nC++\nC#".into())
        );
        assert_eq!(
            tokenizer.prepare("unknown🦀\n:99\nOpen/Dictate\nOpen\0"),
            None
        );
        let vocab = std::fs::read_to_string(&tokenizer.path).unwrap();
        assert!(vocab.contains("▁OPEN\t-1.0"));
    }
}
