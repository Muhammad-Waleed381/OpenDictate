use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum VoiceAction {
    /// Undo the most recent text insertion.
    Undo,
    /// Delete the preceding word (e.g. Ctrl+Backspace).
    DeleteWord,
    /// Delete the current/preceding line.
    DeleteLine,
    /// Clear all text in focused input (Ctrl+A, Backspace).
    ClearAll,
    /// Insert a newline (Enter).
    NewLine,
    /// Insert two newlines (Enter x 2).
    NewParagraph,
    /// Insert a tab indentation (Tab).
    Tab,
    /// Insert a bullet point character (`• `).
    BulletPoint,
    /// Format phrase as ALL CAPS.
    AllCaps(String),
    /// Format phrase as camelCase.
    CamelCase(String),
    /// Format phrase as snake_case.
    SnakeCase(String),
    /// Format phrase as Title Case.
    TitleCase(String),
    /// Insert a saved snippet by trigger/name.
    InsertSnippet(String),
    /// Put Handsfree Mode to sleep.
    Sleep,
    /// Type prompt text and immediately press Enter (for Claude Code, Cursor, ChatGPT, Codex, Terminal).
    PromptAndSend(String),
    /// Submit current focused input (Enter).
    Submit,
    /// Interrupt/cancel running CLI command or process (Ctrl+C).
    Interrupt,
    /// Switch focus or launch application by name (e.g., Cursor, Terminal, Chrome, Slack).
    SwitchApp(String),
    /// Switch to next tab in browser/editor (Ctrl+Tab).
    NextTab,
    /// Switch to previous tab in browser/editor (Ctrl+Shift+Tab).
    PrevTab,
    /// Open new tab (Ctrl+T).
    NewTab,
    /// Close current tab (Ctrl+W).
    CloseTab,
    /// Scroll down one page.
    ScrollDown,
    /// Scroll up one page.
    ScrollUp,
    /// Search Google in default web browser.
    WebSearch(String),
    /// Open website URL in default web browser.
    OpenUrl(String),
    /// Fast developer/terminal command insertion.
    TerminalCommand(String),
}

/// Helper to convert a string to camelCase.
pub fn to_camel_case(input: &str) -> String {
    let input = input.replace(['\'', '\u{2019}'], "");
    let words: Vec<&str> = input
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();

    if words.is_empty() {
        return String::new();
    }

    let mut result = String::new();
    for (i, word) in words.iter().enumerate() {
        let mut chars = word.chars();
        if i == 0 {
            // First word lowercase
            for ch in chars {
                result.extend(ch.to_lowercase());
            }
        } else {
            // Subsequent words capitalize first letter
            if let Some(first) = chars.next() {
                result.extend(first.to_uppercase());
                for ch in chars {
                    result.extend(ch.to_lowercase());
                }
            }
        }
    }
    result
}

/// Helper to convert a string to snake_case.
pub fn to_snake_case(input: &str) -> String {
    let input = input.replace(['\'', '\u{2019}'], "");
    let words: Vec<String> = input
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| w.to_lowercase())
        .collect();

    words.join("_")
}

/// Helper to convert a string to Title Case.
pub fn to_title_case(input: &str) -> String {
    let mut words = Vec::new();
    for word in input.split_whitespace() {
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            let mut capitalized = first.to_uppercase().collect::<String>();
            for ch in chars {
                capitalized.extend(ch.to_lowercase());
            }
            words.push(capitalized);
        }
    }
    words.join(" ")
}

/// Whether a spoken target looks like a URL rather than an application name.
/// Used to route "go to <x>" correctly: "go to example.com" opens a browser,
/// "go to chrome" focuses an application.
fn looks_like_url(value: &str) -> bool {
    let value = value.trim();
    if value.is_empty() {
        return false;
    }
    if value.contains("://") || value.starts_with("www.") {
        return true;
    }
    let host = value.split(['/', '?', '#']).next().unwrap_or("");
    let Some((name, tld)) = host.rsplit_once('.') else {
        return false;
    };
    !name.is_empty() && tld.len() >= 2 && tld.chars().all(|ch| ch.is_ascii_alphabetic())
}

/// Sentence punctuation the recognizer appends to a spoken value.
const SENTENCE_PUNCTUATION: &[char] = &['.', ',', '!', '?', ';', ':', '\u{2026}'];

/// Applications that "open x", "focus x" and "go to x" can name. Those verbs
/// also begin ordinary sentences, so they only act on a name known here; any
/// other application is reached with the unambiguous "switch to" or "open app".
const KNOWN_APPS: &[&str] = &[
    "brave", "browser", "chrome", "code", "console", "cursor", "discord", "edge", "figma",
    "file manager", "files", "finder", "firefox", "google chrome", "notion", "obsidian", "safari",
    "slack", "spotify", "teams", "telegram", "terminal", "visual studio code", "vs code", "vscode",
    "whatsapp", "zoom",
];

fn is_known_app(target: &str) -> bool {
    KNOWN_APPS.contains(&command_key(target).as_str())
}

/// First words that show a "<verb> <rest>" transcript is ordinary prose, such
/// as "open the door" or "go to bed", rather than an application name.
const NOT_APP_NAME: &[&str] = &[
    "a", "about", "all", "an", "and", "any", "at", "bed", "but", "church", "for", "from", "her",
    "here", "him", "his", "home", "if", "in", "it", "me", "my", "of", "on", "or", "our", "out",
    "school", "see", "sleep", "some", "that", "the", "their", "them", "there", "these", "this",
    "those", "to", "up", "us", "when", "with", "work", "you", "your",
];
const NOT_CASE_PHRASE: &[&str] = &[
    "and",
    "are",
    "characters",
    "for",
    "in",
    "is",
    "letter",
    "letters",
    "means",
    "names",
    "naming",
    "or",
    "variables",
    "versus",
    "vs",
    "was",
    "with",
];

fn first_word_is_any(value: &str, words: &[&str]) -> bool {
    value
        .split_whitespace()
        .next()
        .map(|word| {
            word.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .is_some_and(|word| words.contains(&word.as_str()))
}

/// A spoken value without the sentence punctuation the recognizer appended.
fn spoken_value(rest: &str) -> &str {
    rest.trim()
        .trim_end_matches(SENTENCE_PUNCTUATION)
        .trim_end()
}

/// Application names are at most a few words ("visual studio code").
fn is_app_target(target: &str) -> bool {
    !target.is_empty()
        && target.split_whitespace().count() <= 3
        && !first_word_is_any(target, NOT_APP_NAME)
}

/// Turns a spoken URL into its written form: "example dot com slash docs"
/// becomes "example.com/docs". Only sentence punctuation is removed from a URL
/// that was already written out, because `!` and `?` can be part of one. A
/// spoken value that is still not a URL returns `None`.
fn normalize_spoken_url(rest: &str) -> Option<String> {
    let value = rest
        .trim()
        .trim_end_matches(['.', ',', ';', '\u{2026}'])
        .trim_end();
    if value.is_empty() {
        return None;
    }
    if !value.contains(char::is_whitespace) {
        return Some(value.to_string());
    }
    let joined: String = value
        .split_whitespace()
        .map(|word| match word.to_lowercase().as_str() {
            "dot" => ".",
            "slash" => "/",
            "dash" | "hyphen" => "-",
            _ => word,
        })
        .collect();
    looks_like_url(&joined).then_some(joined)
}

/// The form of a transcript used to compare it with whole-utterance commands
/// and wake phrases: lowercase, no surrounding punctuation, and single spaces.
/// Recognizers differ in how they write these ("Scroll-down.", "NEW LINE").
pub fn command_key(raw: &str) -> String {
    raw.trim_matches(|c: char| !c.is_alphanumeric())
        .replace('-', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Parses a transcription text to check if it represents an in-place voice command.
///
/// If it matches a recognized command pattern, returns `Some(VoiceAction)`.
/// If it is normal speech, returns `None`.
pub fn parse_voice_action(raw_text: &str) -> Option<VoiceAction> {
    let cleaned = command_key(raw_text);

    if cleaned.is_empty() {
        return None;
    }

    // 1. Direct single-action commands
    match cleaned.as_str() {
        "scratch that" | "undo that" | "undo" | "revert that" => return Some(VoiceAction::Undo),
        "delete last word" | "delete word" | "erase word" => return Some(VoiceAction::DeleteWord),
        "delete last line" | "delete line" | "clear line" => return Some(VoiceAction::DeleteLine),
        "clear all" | "delete all" | "select all and delete" => return Some(VoiceAction::ClearAll),
        "new line" | "newline" | "press enter" | "hit enter" | "line break" => {
            return Some(VoiceAction::NewLine)
        }
        "new paragraph" | "next paragraph" => return Some(VoiceAction::NewParagraph),
        "tab" | "press tab" | "indent" => return Some(VoiceAction::Tab),
        "bullet point" | "bullet list" | "add bullet" | "bullet" => {
            return Some(VoiceAction::BulletPoint)
        }
        "stop listening" | "go to sleep" | "pause dictation" | "sleep mode" => {
            return Some(VoiceAction::Sleep)
        }
        "send" | "submit" | "send message" | "send prompt" | "submit prompt" => {
            return Some(VoiceAction::Submit)
        }
        "stop process" | "cancel command" | "cancel process" | "abort command" | "interrupt" => {
            return Some(VoiceAction::Interrupt)
        }
        "next tab" | "switch to next tab" => return Some(VoiceAction::NextTab),
        "previous tab" | "prev tab" | "switch to previous tab" => {
            return Some(VoiceAction::PrevTab)
        }
        "new tab" | "open new tab" => return Some(VoiceAction::NewTab),
        "close tab" => return Some(VoiceAction::CloseTab),
        "scroll down" | "page down" => return Some(VoiceAction::ScrollDown),
        "scroll up" | "page up" => return Some(VoiceAction::ScrollUp),
        "git status" => return Some(VoiceAction::TerminalCommand("git status".to_string())),
        "git diff" => return Some(VoiceAction::TerminalCommand("git diff".to_string())),
        "git pull" => return Some(VoiceAction::TerminalCommand("git pull".to_string())),
        "git push" => return Some(VoiceAction::TerminalCommand("git push".to_string())),
        "git log" => return Some(VoiceAction::TerminalCommand("git log".to_string())),
        "clear terminal" | "clear screen" => {
            return Some(VoiceAction::TerminalCommand("clear".to_string()))
        }
        _ => {}
    }

    // Only command words are normalized. Prefix lengths are ASCII byte
    // boundaries checked with `get`, so Unicode payloads remain untouched.
    let original = raw_text
        .trim_start_matches(|c: char| !c.is_alphanumeric())
        .trim_end();
    // A command prefix is the word itself; the payload follows a space, any
    // other whitespace, or a colon. The prefix word is matched
    // case-insensitively, the payload is not.
    let payload = |prefix: &str| -> Option<&str> {
        let word = prefix.trim_end();
        let head = original.get(..word.len())?;
        if !head.eq_ignore_ascii_case(word) {
            return None;
        }
        let rest = original.get(word.len()..)?;
        rest.strip_prefix(|ch: char| matches!(ch, ' ' | '\t' | '\n' | '\r' | ':' | ','))
    };

    // Prefix commands are phrases that do not begin ordinary sentences. One-word
    // openers such as "ask", "search" or "open" are not enough on their own: no
    // list of exceptions can tell "ask the model why" from a request.

    // 2. AI Prompting & Auto-Send: "send prompt <text>", "ask claude <text>"
    if let Some(rest) = [
        "send prompt ",
        "prompt and send ",
        "tell claude ",
        "tell cursor ",
        "tell codex ",
        "tell chatgpt ",
        "ask claude ",
        "ask cursor ",
        "ask codex ",
        "ask chatgpt ",
    ]
    .iter()
    .find_map(|prefix| payload(prefix))
    {
        let prompt_text = rest.trim();
        if !prompt_text.is_empty() {
            return Some(VoiceAction::PromptAndSend(prompt_text.to_string()));
        }
    }

    // 3. Open Website: "open website <url>", "open url <url>"
    if let Some(rest) = payload("open website ")
        .or_else(|| payload("open site "))
        .or_else(|| payload("open url "))
    {
        return normalize_spoken_url(rest).map(VoiceAction::OpenUrl);
    }

    // 3b. "go to <x>" and "open <x>" open a URL when <x> looks like one, and
    // otherwise focus an application they know by name.
    if let Some(rest) = payload("go to ").or_else(|| payload("open ")) {
        if let Some(url) = normalize_spoken_url(rest).filter(|url| looks_like_url(url)) {
            return Some(VoiceAction::OpenUrl(url));
        }
    }

    // 4. Web Search: "search for <query>", "google for <query>"
    if let Some(rest) = payload("search for ").or_else(|| payload("google for ")) {
        let query = spoken_value(rest);
        if !query.is_empty() {
            return Some(VoiceAction::WebSearch(query.to_string()));
        }
    }

    // 5. App Switching: "switch to <app>", "open app <app>" name any application.
    if let Some(rest) = payload("switch to ").or_else(|| payload("open app ")) {
        let target = spoken_value(rest);
        return is_app_target(target).then(|| VoiceAction::SwitchApp(target.to_string()));
    }
    if let Some(rest) = payload("go to ")
        .or_else(|| payload("open "))
        .or_else(|| payload("focus "))
    {
        let target = spoken_value(rest);
        return is_known_app(target).then(|| VoiceAction::SwitchApp(target.to_string()));
    }

    let case_phrase = |prefix: &str| -> Option<&str> {
        payload(prefix)
            .map(spoken_value)
            .filter(|phrase| !phrase.is_empty() && !first_word_is_any(phrase, NOT_CASE_PHRASE))
    };

    // 6. Prefix commands: "all caps <phrase>"
    if let Some(phrase) = case_phrase("all caps ") {
        return Some(VoiceAction::AllCaps(phrase.to_uppercase()));
    }

    // 7. Prefix commands: "camel case <phrase>"
    if let Some(phrase) = case_phrase("camel case ") {
        return Some(VoiceAction::CamelCase(to_camel_case(phrase)));
    }

    // 8. Prefix commands: "snake case <phrase>"
    if let Some(phrase) = case_phrase("snake case ") {
        return Some(VoiceAction::SnakeCase(to_snake_case(phrase)));
    }

    // 9. Prefix commands: "title case <phrase>"
    if let Some(phrase) = case_phrase("title case ") {
        return Some(VoiceAction::TitleCase(to_title_case(phrase)));
    }

    // 10. Prefix commands: "insert snippet <name>", "paste snippet <name>"
    if let Some(rest) = payload("insert snippet ").or_else(|| payload("paste snippet ")) {
        let name = rest.trim();
        if !name.is_empty() {
            return Some(VoiceAction::InsertSnippet(name.to_string()));
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_command_payload_case_punctuation_and_unicode() {
        assert_eq!(
            parse_voice_action("OPEN URL https://Example.com/Case?Token=AbC!"),
            Some(VoiceAction::OpenUrl(
                "https://Example.com/Case?Token=AbC!".into()
            ))
        );
        assert_eq!(
            parse_voice_action("ASK CLAUDE Explain İ and Café?"),
            Some(VoiceAction::PromptAndSend("Explain İ and Café?".into()))
        );
        assert_eq!(
            parse_voice_action("Insert Snippet My API!"),
            Some(VoiceAction::InsertSnippet("My API!".into()))
        );
    }

    #[test]
    fn parses_undo_and_deletion_actions() {
        assert_eq!(parse_voice_action("scratch that."), Some(VoiceAction::Undo));
        assert_eq!(parse_voice_action("Undo that"), Some(VoiceAction::Undo));
        assert_eq!(
            parse_voice_action("delete last word"),
            Some(VoiceAction::DeleteWord)
        );
        assert_eq!(
            parse_voice_action("delete line!"),
            Some(VoiceAction::DeleteLine)
        );
        assert_eq!(
            parse_voice_action("clear all?"),
            Some(VoiceAction::ClearAll)
        );
    }

    #[test]
    fn parses_formatting_actions() {
        assert_eq!(parse_voice_action("new line."), Some(VoiceAction::NewLine));
        assert_eq!(
            parse_voice_action("press enter"),
            Some(VoiceAction::NewLine)
        );
        assert_eq!(
            parse_voice_action("new paragraph"),
            Some(VoiceAction::NewParagraph)
        );
        assert_eq!(parse_voice_action("tab"), Some(VoiceAction::Tab));
        assert_eq!(
            parse_voice_action("bullet point"),
            Some(VoiceAction::BulletPoint)
        );
        assert_eq!(parse_voice_action("go to sleep"), Some(VoiceAction::Sleep));
    }

    #[test]
    fn parses_casing_transformations() {
        assert_eq!(
            parse_voice_action("all caps warning message"),
            Some(VoiceAction::AllCaps("WARNING MESSAGE".to_string()))
        );
        assert_eq!(
            parse_voice_action("camel case get user data"),
            Some(VoiceAction::CamelCase("getUserData".to_string()))
        );
        assert_eq!(
            parse_voice_action("snake case total active users"),
            Some(VoiceAction::SnakeCase("total_active_users".to_string()))
        );
        assert_eq!(
            parse_voice_action("title case user authentication controller"),
            Some(VoiceAction::TitleCase(
                "User Authentication Controller".to_string()
            ))
        );
    }

    #[test]
    fn parses_snippet_macros() {
        assert_eq!(
            parse_voice_action("insert snippet email signature"),
            Some(VoiceAction::InsertSnippet("email signature".to_string()))
        );
        assert_eq!(
            parse_voice_action("paste snippet meeting notes"),
            Some(VoiceAction::InsertSnippet("meeting notes".to_string()))
        );
    }

    #[test]
    fn parses_ai_prompt_and_send() {
        assert_eq!(
            parse_voice_action("send prompt write a test for my api"),
            Some(VoiceAction::PromptAndSend(
                "write a test for my api".to_string()
            ))
        );
        assert_eq!(
            parse_voice_action("tell claude refactor this function"),
            Some(VoiceAction::PromptAndSend(
                "refactor this function".to_string()
            ))
        );
        assert_eq!(parse_voice_action("send"), Some(VoiceAction::Submit));
        assert_eq!(
            parse_voice_action("stop process"),
            Some(VoiceAction::Interrupt)
        );
    }

    #[test]
    fn parses_app_switching_and_nav() {
        assert_eq!(
            parse_voice_action("switch to cursor"),
            Some(VoiceAction::SwitchApp("cursor".to_string()))
        );
        assert_eq!(
            parse_voice_action("open app chrome"),
            Some(VoiceAction::SwitchApp("chrome".to_string()))
        );
        assert_eq!(parse_voice_action("next tab"), Some(VoiceAction::NextTab));
        assert_eq!(parse_voice_action("close tab"), Some(VoiceAction::CloseTab));
        assert_eq!(
            parse_voice_action("scroll down"),
            Some(VoiceAction::ScrollDown)
        );
    }

    #[test]
    fn parses_search_and_url() {
        assert_eq!(
            parse_voice_action("search for best mechanical keyboard"),
            Some(VoiceAction::WebSearch(
                "best mechanical keyboard".to_string()
            ))
        );
        assert_eq!(
            parse_voice_action("google for rust tokio tutorial"),
            Some(VoiceAction::WebSearch("rust tokio tutorial".to_string()))
        );
        assert_eq!(
            parse_voice_action("open website github.com"),
            Some(VoiceAction::OpenUrl("github.com".to_string()))
        );
    }

    #[test]
    fn go_to_routes_urls_and_applications_correctly() {
        assert_eq!(
            parse_voice_action("go to example.com"),
            Some(VoiceAction::OpenUrl("example.com".to_string()))
        );
        assert_eq!(
            parse_voice_action("go to https://example.com/docs"),
            Some(VoiceAction::OpenUrl("https://example.com/docs".to_string()))
        );
        assert_eq!(
            parse_voice_action("go to chrome"),
            Some(VoiceAction::SwitchApp("chrome".to_string()))
        );
    }

    #[test]
    fn command_prefixes_accept_any_whitespace_separator() {
        assert_eq!(
            parse_voice_action("send prompt\twrite a test"),
            Some(VoiceAction::PromptAndSend("write a test".to_string()))
        );
        assert_eq!(
            parse_voice_action("ask claude: explain this"),
            Some(VoiceAction::PromptAndSend("explain this".to_string()))
        );
        assert_eq!(
            parse_voice_action("switch to\tcursor"),
            Some(VoiceAction::SwitchApp("cursor".to_string()))
        );
        // A prefix that is only a prefix of a word must not match.
        assert_eq!(parse_voice_action("prompts are great"), None);
    }

    #[test]
    fn parses_developer_terminal_commands() {
        assert_eq!(
            parse_voice_action("git status"),
            Some(VoiceAction::TerminalCommand("git status".to_string()))
        );
        assert_eq!(
            parse_voice_action("clear terminal"),
            Some(VoiceAction::TerminalCommand("clear".to_string()))
        );
    }

    #[test]
    fn recognizer_punctuation_does_not_break_commands() {
        assert_eq!(
            parse_voice_action("Scroll-down."),
            Some(VoiceAction::ScrollDown)
        );
        assert_eq!(
            parse_voice_action("\"Undo\u{2026}\""),
            Some(VoiceAction::Undo)
        );
        assert_eq!(parse_voice_action("Newline."), Some(VoiceAction::NewLine));
        assert_eq!(
            parse_voice_action("Switch to Chrome."),
            Some(VoiceAction::SwitchApp("Chrome".into()))
        );
        assert_eq!(
            parse_voice_action("Search for rust tutorials."),
            Some(VoiceAction::WebSearch("rust tutorials".into()))
        );
        assert_eq!(
            parse_voice_action("Camel case, get user's data."),
            Some(VoiceAction::CamelCase("getUsersData".into()))
        );
        assert_eq!(
            parse_voice_action("Snake case total, active users."),
            Some(VoiceAction::SnakeCase("total_active_users".into()))
        );
        assert_eq!(
            parse_voice_action("All caps warning message."),
            Some(VoiceAction::AllCaps("WARNING MESSAGE".into()))
        );
        // Separate sentences are not one command.
        assert_eq!(parse_voice_action("Delete. All."), None);
    }

    #[test]
    fn spoken_and_punctuated_urls_open_as_urls() {
        for spoken in [
            "Go to example.com.",
            "Go to example dot com.",
            "open website example dot com",
        ] {
            assert_eq!(
                parse_voice_action(spoken),
                Some(VoiceAction::OpenUrl("example.com".into())),
                "{spoken}"
            );
        }
        assert_eq!(
            parse_voice_action("open site docs dot rs slash tokio"),
            Some(VoiceAction::OpenUrl("docs.rs/tokio".into()))
        );
        assert_eq!(parse_voice_action("open website my favorite place"), None);
    }

    #[test]
    fn ordinary_sentences_are_not_commands() {
        for prose in [
            "Ask him to call me tomorrow.",
            "Ask the team about it.",
            "Prompt engineering is hard.",
            "Open the door.",
            "Open Chrome and check all of the email.",
            "Focus on the task at hand.",
            "Go to bed early.",
            "Go to sleep now.",
            "Go to the store and buy milk.",
            "Google is a large company.",
            "Search results were good.",
            "Camel case is a naming convention.",
            "Uppercase letters are hard to read.",
            "Switch to the next slide.",
            "Ask the model why.",
            "Prompt him to answer.",
            "Search engines changed everything.",
            "Google announced a new phone.",
            "Open your books.",
            "Focus groups disagreed.",
            "Go to market early.",
            "Uppercase is hard to read.",
        ] {
            assert_eq!(parse_voice_action(prose), None, "{prose}");
        }
    }

    #[test]
    fn unambiguous_phrases_run_and_open_reaches_urls_and_known_apps() {
        assert_eq!(
            parse_voice_action("Prompt and send fix the test"),
            Some(VoiceAction::PromptAndSend("fix the test".into()))
        );
        assert_eq!(
            parse_voice_action("Ask ChatGPT why."),
            Some(VoiceAction::PromptAndSend("why.".into()))
        );
        assert_eq!(
            parse_voice_action("Open example.com."),
            Some(VoiceAction::OpenUrl("example.com".into()))
        );
        assert_eq!(
            parse_voice_action("Open VS Code."),
            Some(VoiceAction::SwitchApp("VS Code".into()))
        );
        assert_eq!(
            parse_voice_action("Focus Slack"),
            Some(VoiceAction::SwitchApp("Slack".into()))
        );
        assert_eq!(
            parse_voice_action("Switch to Calculator."),
            Some(VoiceAction::SwitchApp("Calculator".into()))
        );
    }

    #[test]
    fn command_key_normalizes_only_the_written_form() {
        assert_eq!(command_key("  Hey,  Dictate! "), "hey, dictate");
        assert_eq!(command_key("Page-Down."), "page down");
        assert_eq!(command_key("..."), "");
    }

    #[test]
    fn returns_none_for_normal_dictation() {
        assert_eq!(
            parse_voice_action("Hello world this is a normal sentence."),
            None
        );
        assert_eq!(
            parse_voice_action("I want to buy a new line of shoes"),
            None
        );
    }
}
