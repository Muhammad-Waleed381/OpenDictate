pub mod actions;
pub mod polish;
pub use actions::{parse_voice_action, to_camel_case, to_snake_case, to_title_case, VoiceAction};
pub use polish::{polish_text, PolishConfig, PolishMode, PolishProvider};

struct Token {
    start: usize,
    end: usize,
    lower: String,
}

fn tokenize(value: &str) -> Vec<Token> {
    let mut result = Vec::new();
    let mut start = None;
    for (index, character) in value.char_indices() {
        let part_of_word = character.is_alphanumeric() || matches!(character, '\'' | '+' | '#');
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
///
/// # Symbol rules
///
/// A dictionary entry is a *canonical spelling* of what the user wants to see,
/// so its symbols are restored along with its casing:
///
/// * Symbols that sit outside a word (`.` in `.NET`, `™` in `Acme™`) are not
///   part of a token. The entry therefore matches the bare word and the
///   symbol is re-attached: `net` becomes `.NET`, `acme` becomes `Acme™`.
/// * Symbols that are part of a word (`+` and `#`, as in `C++` and `C#`) are
///   token characters. The entry only matches when the source already
///   contains them, so a plain `c` is never rewritten into `C++`.
///
/// A source that already carries the symbols keeps them: `.net` becomes
/// `.NET` and `c++` becomes `C++`.
pub fn correct_dictionary_terms(text: &str, terms: &[String]) -> String {
    #[derive(Clone)]
    struct Term {
        canonical: String,
        tokens: Vec<String>,
        leading: String,
        trailing: String,
    }

    let dictionary: Vec<Term> = terms
        .iter()
        .filter_map(|term| {
            let canonical = term.trim();
            let spans = tokenize(canonical);
            let first = spans.first()?;
            let last = spans.last()?;
            Some(Term {
                canonical: canonical.to_string(),
                leading: canonical[..first.start].to_string(),
                trailing: canonical[last.end..].to_string(),
                tokens: spans.into_iter().map(|token| token.lower).collect(),
            })
        })
        .collect();
    if dictionary.is_empty() {
        return text.to_string();
    }

    let tokens = tokenize(text);
    let mut replacements = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        let matched = dictionary
            .iter()
            .filter_map(|term| {
                let end = index + term.tokens.len();
                (end <= tokens.len()
                    && tokens[index..end]
                        .iter()
                        .map(|token| token.lower.as_str())
                        .eq(term.tokens.iter().map(String::as_str)))
                .then_some((term.tokens.len(), term))
            })
            .max_by_key(|(length, _)| *length);

        if let Some((length, term)) = matched {
            let mut start = tokens[index].start;
            let mut end = tokens[index + length - 1].end;
            let previous_end = replacements.last().map_or(0, |(_, end, _)| *end);
            if let Some(prefix_start) = start.checked_sub(term.leading.len()) {
                if prefix_start >= previous_end
                    && text.get(prefix_start..start) == Some(term.leading.as_str())
                {
                    start = prefix_start;
                }
            }
            if text.get(end..end + term.trailing.len()) == Some(term.trailing.as_str()) {
                end += term.trailing.len();
            }
            replacements.push((start, end, term.canonical.clone()));
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
            replacements.push((
                tokens[index].start,
                tokens[index + length - 1].end,
                symbol,
                prev_taken,
            ));
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

/// Filters out hallucinated Whisper closed-caption sound descriptors, such as:
/// `(Gun firing)`, `(Gunshot)`, `[Applause]`, `[Music]`, `(Wind blowing)`, `(Bell dings)`, `[Laughter]`, `*applause*`.
pub fn strip_sound_effects(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    // Strip inline bracketed and parenthesized annotations
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '[' || ch == '(' || ch == '*' {
            let close_char = match ch {
                '[' => ']',
                '(' => ')',
                '*' => '*',
                _ => unreachable!(),
            };
            let mut tag = String::new();
            let mut closed = false;
            for inner in chars.by_ref() {
                if inner == close_char {
                    closed = true;
                    break;
                }
                tag.push(inner);
            }
            if !closed {
                out.push(ch);
                out.push_str(&tag);
            } else {
                // Exact known annotation phrases only. Arbitrary square
                // brackets and substrings such as "ding" in "coding" are prose.
                let tag_lower = tag
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .to_lowercase();
                let is_sound_effect = matches!(
                    tag_lower.as_str(),
                    "gun firing"
                        | "gunshot"
                        | "gunshots"
                        | "music"
                        | "applause"
                        | "laughter"
                        | "laughing"
                        | "giggle"
                        | "giggles"
                        | "chuckle"
                        | "sigh"
                        | "sighs"
                        | "cough"
                        | "coughing"
                        | "clears throat"
                        | "screaming"
                        | "shouting"
                        | "wind blowing"
                        | "bell dings"
                        | "bell ringing"
                        | "beep"
                        | "beeping"
                        | "click"
                        | "clicking"
                        | "creak"
                        | "creaking"
                        | "silence"
                        | "snicker"
                        | "groan"
                        | "gasp"
                        | "cheering"
                        | "chatter"
                        | "background noise"
                        | "inaudible"
                        | "unintelligible"
                );

                let is_boundary = |ch: char| {
                    ch.is_whitespace() || matches!(ch, '.' | ',' | ';' | ':' | '!' | '?')
                };
                let standalone = out.chars().next_back().is_none_or(is_boundary)
                    && chars.peek().copied().is_none_or(is_boundary);
                if !is_sound_effect || !standalone {
                    out.push(ch);
                    out.push_str(&tag);
                    out.push(close_char);
                } else if out.trim().is_empty() {
                    // The annotation started the text and was removed. A
                    // sentence terminator that followed it belonged to the
                    // annotation's own sentence, so remove it too instead of
                    // leaving a lone "." behind.
                    let mut lookahead = chars.clone();
                    if let Some(next) = lookahead.next() {
                        if matches!(next, '.' | ',' | ';' | ':' | '!' | '?') {
                            let _ = chars.next();
                        }
                    }
                }
            }
        } else {
            out.push(ch);
        }
    }

    out
}

/// Cleans up model hallucination loops where a word or multi-word phrase is repeated
/// consecutively (e.g. "I can do it myself I can do it myself I can do it myself ... I can do"
/// or "Test Test Test Test Test" or "Yes, I do that myself! Yes, I do that myself!").
pub fn deduplicate_repeated_phrases(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let words: Vec<&str> = trimmed.split_whitespace().collect();
    if words.len() < 2 {
        return trimmed.to_string();
    }

    let normalize = |w: &str| -> String {
        w.trim_matches(|c: char| c.is_ascii_punctuation())
            .to_lowercase()
    };

    let norm_words: Vec<String> = words.iter().map(|w| normalize(w)).collect();
    let n = words.len();

    let mut i = 0;
    let mut result_words: Vec<&str> = Vec::new();

    while i < n {
        let mut best_match: Option<(usize, usize, usize)> = None;

        for k in 1..=15.min((n - i) / 2) {
            let pattern = &norm_words[i..i + k];
            if pattern.iter().all(|w| w.is_empty()) {
                continue;
            }

            let mut repeats = 1;
            let mut cursor = i + k;

            while cursor + k <= n && norm_words[cursor..cursor + k] == *pattern {
                repeats += 1;
                cursor += k;
            }

            let mut partial_len = 0;
            for p in (1..k).rev() {
                if cursor + p <= n && norm_words[cursor..cursor + p] == pattern[..p] {
                    partial_len = p;
                    break;
                }
            }

            let min_repeats = if k == 1 { 3 } else { 2 };
            if repeats >= min_repeats {
                best_match = Some((k, repeats, partial_len));
                break;
            }
        }

        if let Some((k, repeats, partial_len)) = best_match {
            result_words.extend_from_slice(&words[i..i + k]);
            i += repeats * k + partial_len;
        } else {
            result_words.push(words[i]);
            i += 1;
        }
    }

    result_words.join(" ")
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
        let chars: Vec<char> = value.chars().collect();
        (0..chars.len().saturating_sub(1))
            .map(|i| [chars[i], chars[i + 1]])
            .collect::<Vec<_>>()
    };
    let ab = bigrams(a);
    let bb = bigrams(b);
    if ab.is_empty() || bb.is_empty() {
        return 0.0;
    }
    let mut available = std::collections::HashMap::new();
    for gram in &bb {
        *available.entry(gram).or_insert(0usize) += 1;
    }
    let overlap = ab
        .iter()
        .filter(|gram| {
            if let Some(count) = available.get_mut(gram) {
                if *count > 0 {
                    *count -= 1;
                    return true;
                }
            }
            false
        })
        .count();
    2.0 * overlap as f32 / (ab.len() + bb.len()) as f32
}

/// Whether `value` is a single standalone word under the tokenizer rules
/// (alphanumerics, apostrophes, `+` and `#`). Used to restrict snippet
/// triggers to single spoken words.
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
pub fn fuzzy_match_trigger(
    spoken: &str,
    triggers: &[String],
    threshold: f32,
) -> Option<(String, f32)> {
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

        if spoken_tokens == trigger_tokens && threshold <= 1.0 {
            return Some((trigger.to_string(), 1.0));
        }
        let cols = trigger_tokens.len() + 1;
        let mut prev: Vec<f32> = (0..cols).map(|j| j as f32).collect();
        for (i, spoken_token) in spoken_tokens.iter().enumerate() {
            let mut cur = vec![0.0f32; cols];
            cur[0] = (i + 1) as f32;
            for j in 1..cols {
                let substitute =
                    prev[j - 1] + 1.0 - char_dice(spoken_token, &trigger_tokens[j - 1]);
                let delete = prev[j] + 1.0;
                let insert = cur[j - 1] + 1.0;
                cur[j] = substitute.min(delete).min(insert);
            }
            prev = cur;
        }

        let max_tokens = spoken_tokens.len().max(trigger_tokens.len());
        let score = (1.0 - prev[cols - 1] / max_tokens as f32).clamp(0.0, 1.0);
        if score >= threshold && best.as_ref().is_none_or(|(_, s)| score > *s) {
            best = Some((trigger.to_string(), score));
        }
    }
    best
}

/// Expands dynamic template variables in snippet text.
///
/// Supported placeholders (case-insensitive):
/// - `{date}` / `{DATE}`: Current local date as `YYYY-MM-DD`
/// - `{time}` / `{TIME}`: Current local time as `HH:MM`
/// - `{datetime}` / `{DATETIME}`: Current local datetime as `YYYY-MM-DD HH:MM`
/// - `{clipboard}` / `{CLIPBOARD}`: Contents of clipboard, or empty string if None/empty
///
/// Any unrecognized placeholders (e.g. `{unknown}`) and malformed braces are preserved as-is.
pub fn expand_snippet_template(template: &str, clipboard: Option<&str>) -> String {
    let now = chrono::Local::now();
    let date_str = now.format("%Y-%m-%d").to_string();
    let time_str = now.format("%H:%M").to_string();
    let datetime_str = now.format("%Y-%m-%d %H:%M").to_string();
    let clipboard_str = clipboard.unwrap_or("");

    let mut result = String::with_capacity(template.len());
    let mut cursor = 0;

    while cursor < template.len() {
        if let Some(open_rel) = template[cursor..].find('{') {
            let open_idx = cursor + open_rel;
            result.push_str(&template[cursor..open_idx]);

            if let Some(close_rel) = template[open_idx + 1..].find('}') {
                let close_idx = open_idx + 1 + close_rel;
                let candidate = &template[open_idx + 1..close_idx];

                if let Some(inner_open_rel) = candidate.rfind('{') {
                    // There is an unclosed '{' before '}' (e.g. "{prefix {date}")
                    let inner_open_idx = open_idx + 1 + inner_open_rel;
                    result.push_str(&template[open_idx..inner_open_idx]);
                    cursor = inner_open_idx;
                } else {
                    if candidate.eq_ignore_ascii_case("date") {
                        result.push_str(&date_str);
                    } else if candidate.eq_ignore_ascii_case("time") {
                        result.push_str(&time_str);
                    } else if candidate.eq_ignore_ascii_case("datetime") {
                        result.push_str(&datetime_str);
                    } else if candidate.eq_ignore_ascii_case("clipboard") {
                        result.push_str(clipboard_str);
                    } else {
                        // Unknown placeholder, preserve verbatim including braces
                        result.push_str(&template[open_idx..=close_idx]);
                    }
                    cursor = close_idx + 1;
                }
            } else {
                // No closing '}' found; append remainder as-is
                result.push_str(&template[open_idx..]);
                break;
            }
        } else {
            result.push_str(&template[cursor..]);
            break;
        }
    }

    result
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
    fn dictionary_preserves_symbol_bearing_terms() {
        assert_eq!(
            correct_dictionary_terms("c++ and c#", &terms(&["C++", "C#"])),
            "C++ and C#"
        );
        assert_eq!(
            correct_dictionary_terms("C and c++.", &terms(&["C++"])),
            "C and C++."
        );
        assert_eq!(
            correct_dictionary_terms(".net and acme™", &terms(&[".NET", "Acme™"])),
            ".NET and Acme™"
        );
    }

    #[test]
    fn dictionary_symbol_rules_are_consistent() {
        // A symbol outside a word is restored from the bare word.
        assert_eq!(correct_dictionary_terms("net", &terms(&[".NET"])), ".NET");
        assert_eq!(
            correct_dictionary_terms("acme", &terms(&["Acme™"])),
            "Acme™"
        );
        // A symbol inside a word is only restored when it was spoken.
        assert_eq!(correct_dictionary_terms("c", &terms(&["C++"])), "c");
        assert_eq!(correct_dictionary_terms("c++", &terms(&["C++"])), "C++");
    }

    #[test]
    fn removed_annotations_do_not_leave_a_dangling_terminator() {
        assert_eq!(super::strip_sound_effects("[Music]."), "");
        assert_eq!(super::strip_sound_effects("(Gun firing)"), "");
        // A terminator that belongs to the surrounding sentence stays.
        assert_eq!(super::strip_sound_effects("Hello [Music]."), "Hello .");
    }

    #[test]
    fn annotations_preserve_normal_prose_and_code() {
        for text in [
            "array[index]",
            "array[music]",
            "[important details]",
            "(coding)",
            "(Windows)",
            "(music theory)",
        ] {
            assert_eq!(super::strip_sound_effects(text), text);
        }
    }

    #[test]
    fn exact_trigger_beats_repeated_bigram_candidate() {
        let (name, score) = fuzzy_match_trigger("aaaaa", &terms(&["aa", "aaaaa"]), 0.0).unwrap();
        assert_eq!(name, "aaaaa");
        assert_eq!(score, 1.0);
        assert!((0.0..=1.0).contains(&super::char_dice("aaaaa", "aa")));
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
        assert_eq!(
            map_spoken_punctuation("three point five"),
            "three point five"
        );
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
        let (name, score) = fuzzy_match_trigger(
            "email signature",
            &triggers(&["email signature", "meeting notes"]),
            0.6,
        )
        .expect("exact trigger should match");
        assert_eq!(name, "email signature");
        assert_eq!(score, 1.0);
    }

    #[test]
    fn fuzzy_matches_case_insensitively() {
        let (name, _) =
            fuzzy_match_trigger("EMAIL SIGNATURE", &triggers(&["Email Signature"]), 0.6)
                .expect("case-insensitive match should succeed");
        assert_eq!(name, "Email Signature");
    }

    #[test]
    fn fuzzy_matches_slight_garbles() {
        let (name, _) = fuzzy_match_trigger(
            "email signure",
            &triggers(&["email signature", "meeting notes"]),
            0.6,
        )
        .expect("near-miss should still match");
        assert_eq!(name, "email signature");
    }

    #[test]
    fn fuzzy_matches_best_of_several() {
        let (name, _) = fuzzy_match_trigger(
            "email signature",
            &triggers(&["meeting notes", "gmail signature"]),
            0.6,
        )
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

    #[test]
    fn strips_hallucinated_sound_effect_tags() {
        use super::strip_sound_effects;
        assert_eq!(strip_sound_effects("(Gun firing)"), "");
        assert_eq!(strip_sound_effects("[Gunshot]"), "");
        assert_eq!(strip_sound_effects("[Applause]"), "");
        assert_eq!(strip_sound_effects("(wind blowing)"), "");
        assert_eq!(strip_sound_effects("*music*"), "");
        assert_eq!(
            strip_sound_effects("hello (applause) world"),
            "hello  world"
        );
        assert_eq!(
            strip_sound_effects("this is (gun firing) great"),
            "this is  great"
        );
        assert_eq!(
            strip_sound_effects("normal text with (important details)"),
            "normal text with (important details)"
        );
    }

    #[test]
    fn test_deduplicate_repeated_phrases() {
        use super::deduplicate_repeated_phrases;
        assert_eq!(
            deduplicate_repeated_phrases("I can do it myself I can do it myself I can do it myself I can do it myself I can do"),
            "I can do it myself"
        );
        assert_eq!(
            deduplicate_repeated_phrases("Yes, I do that myself! Yes, I do that myself!"),
            "Yes, I do that myself!"
        );
        assert_eq!(
            deduplicate_repeated_phrases("Test Test Test Test Test"),
            "Test"
        );
        assert_eq!(
            deduplicate_repeated_phrases("No, no, I think so"),
            "No, no, I think so"
        );
        assert_eq!(
            deduplicate_repeated_phrases("This is a normal sentence."),
            "This is a normal sentence."
        );
    }
}

#[cfg(test)]
mod snippet_template_tests {
    use super::expand_snippet_template;
    use chrono::Local;

    #[test]
    fn expands_date_placeholders() {
        let before = Local::now();
        let result = expand_snippet_template("Date: {date}", None);
        let after = Local::now();
        let expected_before = format!("Date: {}", before.format("%Y-%m-%d"));
        let expected_after = format!("Date: {}", after.format("%Y-%m-%d"));
        assert!(
            result == expected_before || result == expected_after,
            "result {result:?} should match {expected_before:?} or {expected_after:?}"
        );

        let result_upper = expand_snippet_template("Date: {DATE}", None);
        assert!(
            result_upper == expected_before || result_upper == expected_after,
            "result {result_upper:?} should match {expected_before:?} or {expected_after:?}"
        );
    }

    #[test]
    fn expands_time_placeholders() {
        let before = Local::now();
        let result = expand_snippet_template("Time: {time}", None);
        let after = Local::now();
        let expected_before = format!("Time: {}", before.format("%H:%M"));
        let expected_after = format!("Time: {}", after.format("%H:%M"));
        assert!(
            result == expected_before || result == expected_after,
            "result {result:?} should match {expected_before:?} or {expected_after:?}"
        );

        let result_upper = expand_snippet_template("Time: {TIME}", None);
        assert!(
            result_upper == expected_before || result_upper == expected_after,
            "result {result_upper:?} should match {expected_before:?} or {expected_after:?}"
        );
    }

    #[test]
    fn expands_datetime_placeholders() {
        let before = Local::now();
        let result = expand_snippet_template("Now: {datetime}", None);
        let after = Local::now();
        let expected_before = format!("Now: {}", before.format("%Y-%m-%d %H:%M"));
        let expected_after = format!("Now: {}", after.format("%Y-%m-%d %H:%M"));
        assert!(
            result == expected_before || result == expected_after,
            "result {result:?} should match {expected_before:?} or {expected_after:?}"
        );

        let result_upper = expand_snippet_template("Now: {DATETIME}", None);
        assert!(
            result_upper == expected_before || result_upper == expected_after,
            "result {result_upper:?} should match {expected_before:?} or {expected_after:?}"
        );
    }

    #[test]
    fn expands_clipboard_placeholder() {
        assert_eq!(
            expand_snippet_template("Value: {clipboard}", Some("copied text")),
            "Value: copied text"
        );
        assert_eq!(
            expand_snippet_template("Value: {CLIPBOARD}", Some("copied text")),
            "Value: copied text"
        );
    }

    #[test]
    fn handles_empty_or_none_clipboard() {
        assert_eq!(
            expand_snippet_template("Value: {clipboard}", None),
            "Value: "
        );
        assert_eq!(
            expand_snippet_template("Value: {clipboard}", Some("")),
            "Value: "
        );
    }

    #[test]
    fn preserves_unknown_placeholders() {
        assert_eq!(
            expand_snippet_template("Hello {name}, your code is {code}.", None),
            "Hello {name}, your code is {code}."
        );
        assert_eq!(expand_snippet_template("{unknown}", None), "{unknown}");
    }

    #[test]
    fn handles_plain_text_and_empty() {
        assert_eq!(expand_snippet_template("", None), "");
        assert_eq!(
            expand_snippet_template("Simple text without variables", None),
            "Simple text without variables"
        );
    }

    #[test]
    fn handles_malformed_braces() {
        let now = Local::now();
        let expected_date = now.format("%Y-%m-%d").to_string();
        assert_eq!(expand_snippet_template("{", None), "{");
        assert_eq!(expand_snippet_template("}", None), "}");
        assert_eq!(expand_snippet_template("{}", None), "{}");
        assert_eq!(expand_snippet_template("{date", None), "{date");
        assert_eq!(
            expand_snippet_template("{date} {time", None),
            format!("{expected_date} {{time")
        );
    }

    #[test]
    fn does_not_recursively_expand_clipboard() {
        assert_eq!(
            expand_snippet_template("Clip: {clipboard}", Some("{date} and {time}")),
            "Clip: {date} and {time}"
        );
    }

    #[test]
    fn handles_multiple_and_mixed_placeholders() {
        let now = Local::now();
        let expected_date = now.format("%Y-%m-%d").to_string();
        let expected_time = now.format("%H:%M").to_string();
        let template = "[{date} {time}] {clipboard} - Status: {status}";
        let result = expand_snippet_template(template, Some("Task finished"));
        assert_eq!(
            result,
            format!("[{expected_date} {expected_time}] Task finished - Status: {{status}}")
        );
    }

    #[test]
    fn handles_unicode_characters() {
        let now = Local::now();
        let expected_date = now.format("%Y-%m-%d").to_string();
        let template = "Hej världen! 🚀 Idag är {date}, innehåll: {clipboard}";
        let result = expand_snippet_template(template, Some("Café ☕"));
        assert_eq!(
            result,
            format!("Hej världen! 🚀 Idag är {expected_date}, innehåll: Café ☕")
        );
    }
}
