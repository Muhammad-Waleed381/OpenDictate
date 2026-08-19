/// Applies dictionary casing to matching words and phrases without touching
/// substrings inside unrelated words.
pub fn correct_dictionary_terms(text: &str, terms: &[String]) -> String {
    #[derive(Clone)]
    struct Term {
        canonical: String,
        tokens: Vec<String>,
    }

    #[derive(Clone)]
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

#[cfg(test)]
mod tests {
    use super::correct_dictionary_terms;

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
}
