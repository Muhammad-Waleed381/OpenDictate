struct Token {
    start: usize,
    end: usize,
    lower: String,
}

fn tokenize(value: &str) -> Vec<Token> {
    let mut result = Vec::new();
    let mut start = None;
    for (index, character) in value.char_indices() {
        let part_of_word = character.is_alphanumeric() || character == '\'';
        match (start, part_of_word) {
            (None, true) => start = Some(index),
            (Some(begin), false) => {
                let end = index;
                result.push(Token {
                    start: begin,
                    end,
                    lower: value[begin..end].to_lowercase(),
                });
                start = None;
            }
            _ => {}
        }
    }
    if let Some(begin) = start {
        result.push(Token {
            start: begin,
            end: value.len(),
            lower: value[begin..].to_lowercase(),
        });
    }
    result
}

/// Applies dictionary casing to matching words and phrases without touching
/// substrings inside unrelated words.
pub fn correct_dictionary_terms(text: &str, terms: &[String]) -> String {
    #[derive(Clone)]
    struct Term {
        canonical: String,
        tokens: Vec<String>,
    }

    let dictionary: Vec<Term> = terms
        .iter()
        .filter_map(|term| {
            let canonical = term.trim();
            let tokens = tokenize(canonical)
                .into_iter()
                .map(|token| token.lower)
                .collect::<Vec<_>>();
            if canonical.is_empty() || tokens.is_empty() {
                None
            } else {
                Some(Term {
                    canonical: canonical.to_string(),
                    tokens,
                })
            }
        })
        .collect();
    if dictionary.is_empty() {
        return text.to_string();
    }

    let tokens = tokenize(text);
    let mut replacements = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        let matched = dictionary.iter().filter_map(|term| {
            let end = index + term.tokens.len();
            (end <= tokens.len()
                && tokens[index..end]
                    .iter()
                    .map(|token| token.lower.as_str())
                    .eq(term.tokens.iter().map(String::as_str)))
                .then_some((term.tokens.len(), term))
        }).max_by_key(|(length, _)| *length);

        if let Some((length, term)) = matched {
            replacements.push((
                tokens[index].start,
                tokens[index + length - 1].end,
                term.canonical.clone(),
            ));
            index += length;
        } else {
            index += 1;
        }
    }

    let mut corrected = String::with_capacity(text.len());
    let mut cursor = 0;
    for (start, end, replacement) in replacements {
        corrected.push_str(&text[cursor..start]);
        corrected.push_str(&replacement);
        cursor = end;
    }
    corrected.push_str(&text[cursor..]);
    corrected
}

/// Maps spoken punctuation words to their symbols. Case-insensitive,
/// standalone-token only — "point" is preserved so decimals survive.
/// Each symbol attaches to the preceding word (trailing gap whitespace is
/// trimmed), while whitespace between consecutive mapped symbols is preserved.
pub fn map_spoken_punctuation(text: &str) -> String {
    const PHRASES: &[(&[&str], char)] = &[
        (&["period"], '.'),
        (&["comma"], ','),
        (&["question", "mark"], '?'),
        (&["exclamation", "point"], '!'),
        (&["exclamation", "mark"], '!'),
    ];

    let tokens = tokenize(text);
    let mut replacements: Vec<(usize, usize, char, bool)> = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        let matched = PHRASES
            .iter()
            .filter_map(|(phrase, symbol)| {
                let end = index + phrase.len();
                (end <= tokens.len()
                    && tokens[index..end]
                        .iter()
                        .map(|token| token.lower.as_str())
                        .eq(phrase.iter().copied()))
                    .then_some((phrase.len(), *symbol))
            })
            .max_by_key(|(length, _)| *length);

        if let Some((length, symbol)) = matched {
            // Previous replacement ended exactly at the token before this match,
            // so the inter-symbol gap must be preserved.
            let prev_taken = index > 0
                && !replacements.is_empty()
                && tokens[index - 1].end == replacements.last().unwrap().1;
            replacements.push((tokens[index].start, tokens[index + length - 1].end, symbol, prev_taken));
            index += length;
        } else {
            index += 1;
        }
    }

    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    for (start, end, symbol, prev_taken) in replacements {
        let gap = &text[cursor..start];
        if prev_taken {
            out.push_str(gap);
        } else {
            out.push_str(gap.trim_end());
        }
        out.push(symbol);
        cursor = end;
        // The model may already have emitted the symbol after the spoken
        // word (e.g. Whisper outputs "period.") — drop the duplicate.
        let rest = &text[cursor..];
        let skipped = rest.trim_start();
        if skipped.starts_with(symbol) {
            cursor += (rest.len() - skipped.len()) + symbol.len_utf8();
        }
    }
    out.push_str(&text[cursor..]);
    out
}

/// Dice coefficient over lowercase character bigrams for a single token pair.
fn char_dice(a: &str, b: &str) -> f32 {
    if a == b {
        return 1.0;
    }
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let bigrams = |value: &str| {
        let bytes = value.as_bytes();
        (0..bytes.len().saturating_sub(1))
            .map(|i| [bytes[i], bytes[i + 1]])
            .collect::<Vec<_>>()
    };
    let ab = bigrams(a);
    let bb = bigrams(b);
    if ab.is_empty() || bb.is_empty() {
        return 0.0;
    }
    let overlap = ab.iter().filter(|g| bb.contains(g)).count();
    2.0 * overlap as f32 / (ab.len() + bb.len()) as f32
}

/// Whether `value` is a single standalone word under the tokenizer rules
/// (alphanumerics and apostrophes). Used to restrict snippet triggers to
/// single spoken words.
pub fn is_single_word(value: &str) -> bool {
    tokenize(value).len() == 1
}

/// Best-effort match of a spoken snippet name against known triggers.
///
/// Aligns the token sequences with an edit-distance DP whose substitution
/// cost is the complement of the per-token character similarity, so a single
/// garbled word (e.g. "signure" vs "signature") still scores highly while
/// unrelated names score near zero. Returns the best candidate above
/// `threshold` together with its score.
pub fn fuzzy_match_trigger(spoken: &str, triggers: &[String], threshold: f32) -> Option<(String, f32)> {
    let spoken_tokens = tokenize(spoken)
        .into_iter()
        .map(|token| token.lower)
        .collect::<Vec<_>>();
    if spoken_tokens.is_empty() {
        return None;
    }

    let mut best: Option<(String, f32)> = None;
    for trigger in triggers {
        let trigger = trigger.trim();
        if trigger.is_empty() {
            continue;
        }
        let trigger_tokens = tokenize(trigger)
            .into_iter()
            .map(|token| token.lower)
            .collect::<Vec<_>>();
        if trigger_tokens.is_empty() {
            continue;
        }

        let cols = trigger_tokens.len() + 1;
        let mut prev: Vec<f32> = (0..cols).map(|j| j as f32).collect();
        for (i, spoken_token) in spoken_tokens.iter().enumerate() {
            let mut cur = vec![0.0f32; cols];
            cur[0] = (i + 1) as f32;
            for j in 1..cols {
                let substitute = prev[j - 1]
                    + 1.0
                    - char_dice(spoken_token, &trigger_tokens[j - 1]);
                let delete = prev[j] + 1.0;
                let insert = cur[j - 1] + 1.0;
                cur[j] = substitute.min(delete).min(insert);
            }
            prev = cur;
        }

        let max_tokens = spoken_tokens.len().max(trigger_tokens.len());
        let score = 1.0 - prev[cols - 1] / max_tokens as f32;
        if score >= threshold && best.as_ref().is_none_or(|(_, s)| score > *s) {
            best = Some((trigger.to_string(), score));
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::correct_dictionary_terms;
    use super::fuzzy_match_trigger;
    use super::is_single_word;
    use super::map_spoken_punctuation;

    fn terms(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn restores_dictionary_casing_for_words() {
        let result = correct_dictionary_terms("i use iphone every day", &terms(&["iPhone"]));
        assert_eq!(result, "i use iPhone every day");
    }

    #[test]
    fn restores_casing_for_multi_word_phrases() {
        let result = correct_dictionary_terms("we met in new york", &terms(&["New York"]));
        assert_eq!(result, "we met in New York");
    }

    #[test]
    fn does_not_replace_substrings() {
        let result = correct_dictionary_terms("iphoneography is unrelated", &terms(&["iPhone"]));
        assert_eq!(result, "iphoneography is unrelated");
    }

    #[test]
    fn maps_all_core_punctuation_words() {
        assert_eq!(
            map_spoken_punctuation("period comma question mark exclamation point exclamation mark"),
            ". , ? ! !"
        );
    }

    #[test]
    fn maps_punctuation_mid_sentence() {
        assert_eq!(
            map_spoken_punctuation("hello period this is important comma right question mark"),
            "hello. this is important, right?"
        );
    }

    #[test]
    fn preserves_point_in_decimals() {
        assert_eq!(map_spoken_punctuation("three point five"), "three point five");
    }

    #[test]
    fn does_not_match_inside_words() {
        assert_eq!(map_spoken_punctuation("periodontist"), "periodontist");
    }

    #[test]
    fn matching_is_case_insensitive() {
        assert_eq!(map_spoken_punctuation("Period Comma"), ". ,");
    }

    #[test]
    fn preserves_gap_between_consecutive_identical_symbols() {
        assert_eq!(map_spoken_punctuation("period period"), ". .");
    }

    #[test]
    fn trims_leading_whitespace_and_preserves_trailing() {
        assert_eq!(map_spoken_punctuation("  period  "), ".  ");
    }

    #[test]
    fn collapses_duplicate_symbol_when_model_already_punctuated() {
        assert_eq!(
            map_spoken_punctuation("This is just a test of period. What is this?"),
            "This is just a test of. What is this?"
        );
    }

    #[test]
    fn collapses_duplicate_symbol_separated_by_whitespace() {
        assert_eq!(map_spoken_punctuation("go period ."), "go.");
        assert_eq!(map_spoken_punctuation("really question mark ?"), "really?");
    }

    fn triggers(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn fuzzy_matches_exact_trigger() {
        let (name, score) = fuzzy_match_trigger("email signature", &triggers(&["email signature", "meeting notes"]), 0.6)
            .expect("exact trigger should match");
        assert_eq!(name, "email signature");
        assert_eq!(score, 1.0);
    }

    #[test]
    fn fuzzy_matches_case_insensitively() {
        let (name, _) = fuzzy_match_trigger("EMAIL SIGNATURE", &triggers(&["Email Signature"]), 0.6)
            .expect("case-insensitive match should succeed");
        assert_eq!(name, "Email Signature");
    }

    #[test]
    fn fuzzy_matches_slight_garbles() {
        let (name, _) = fuzzy_match_trigger("email signure", &triggers(&["email signature", "meeting notes"]), 0.6)
            .expect("near-miss should still match");
        assert_eq!(name, "email signature");
    }

    #[test]
    fn fuzzy_matches_best_of_several() {
        let (name, _) = fuzzy_match_trigger("email signature", &triggers(&["meeting notes", "gmail signature"]), 0.6)
            .expect("closest candidate should match");
        assert_eq!(name, "gmail signature");
    }

    #[test]
    fn fuzzy_rejects_unrelated_names() {
        assert_eq!(
            fuzzy_match_trigger("banana bread recipe", &triggers(&["email signature"]), 0.6),
            None
        );
    }

    #[test]
    fn fuzzy_rejects_single_token_when_under_threshold() {
        assert_eq!(
            fuzzy_match_trigger("banana", &triggers(&["email signature"]), 0.6),
            None
        );
    }

    #[test]
    fn single_word_detection() {
        assert!(is_single_word("signature"));
        assert!(is_single_word("don't"));
        assert!(is_single_word("Signature"));
        assert!(!is_single_word("email signature"));
        assert!(!is_single_word(""));
        assert!(!is_single_word("follow-up"));
    }

    #[test]
    fn fuzzy_matches_garbled_single_word_trigger() {
        let (name, _) = fuzzy_match_trigger("sginature", &triggers(&["signature", "notes"]), 0.6)
            .expect("garbled single word should still match");
        assert_eq!(name, "signature");
    }
}
