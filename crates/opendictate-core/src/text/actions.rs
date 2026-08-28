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
    let words: Vec<&str> = input
        .split(|c: char| c.is_whitespace() || c == '_' || c == '-')
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
    let words: Vec<String> = input
        .split(|c: char| c.is_whitespace() || c == '_' || c == '-')
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

/// Parses a transcription text to check if it represents an in-place voice command.
///
/// If it matches a recognized command pattern, returns `Some(VoiceAction)`.
/// If it is normal speech, returns `None`.
pub fn parse_voice_action(raw_text: &str) -> Option<VoiceAction> {
    let cleaned = raw_text
        .trim()
        .trim_end_matches(|c: char| c == '.' || c == '!' || c == '?' || c == ',')
        .trim()
        .to_lowercase();

    if cleaned.is_empty() {
        return None;
    }

    // 1. Direct single-action commands
    match cleaned.as_str() {
        "scratch that" | "undo that" | "undo" | "revert that" => return Some(VoiceAction::Undo),
        "delete last word" | "delete word" | "erase word" => return Some(VoiceAction::DeleteWord),
        "delete last line" | "delete line" | "clear line" => return Some(VoiceAction::DeleteLine),
        "clear all" | "delete all" | "select all and delete" => return Some(VoiceAction::ClearAll),
        "new line" | "press enter" | "hit enter" | "line break" => return Some(VoiceAction::NewLine),
        "new paragraph" | "next paragraph" => return Some(VoiceAction::NewParagraph),
        "tab" | "press tab" | "indent" => return Some(VoiceAction::Tab),
        "bullet point" | "bullet list" | "add bullet" | "bullet" => return Some(VoiceAction::BulletPoint),
        "stop listening" | "go to sleep" | "pause dictation" | "sleep mode" => return Some(VoiceAction::Sleep),
        "send" | "submit" | "send message" | "send prompt" | "submit prompt" => return Some(VoiceAction::Submit),
        "stop process" | "cancel command" | "cancel process" | "abort command" | "interrupt" => return Some(VoiceAction::Interrupt),
        "next tab" | "switch to next tab" => return Some(VoiceAction::NextTab),
        "previous tab" | "prev tab" | "switch to previous tab" => return Some(VoiceAction::PrevTab),
        "new tab" | "open new tab" => return Some(VoiceAction::NewTab),
        "close tab" => return Some(VoiceAction::CloseTab),
        "scroll down" | "page down" => return Some(VoiceAction::ScrollDown),
        "scroll up" | "page up" => return Some(VoiceAction::ScrollUp),
        "git status" => return Some(VoiceAction::TerminalCommand("git status".to_string())),
        "git diff" => return Some(VoiceAction::TerminalCommand("git diff".to_string())),
        "git pull" => return Some(VoiceAction::TerminalCommand("git pull".to_string())),
        "git push" => return Some(VoiceAction::TerminalCommand("git push".to_string())),
        "git log" => return Some(VoiceAction::TerminalCommand("git log".to_string())),
        "clear terminal" | "clear screen" => return Some(VoiceAction::TerminalCommand("clear".to_string())),
        _ => {}
    }

    // 2. AI Prompting & Auto-Send: "prompt <text>", "ask <text>"
    if let Some(rest) = cleaned.strip_prefix("prompt ")
        .or_else(|| cleaned.strip_prefix("ask "))
        .or_else(|| cleaned.strip_prefix("tell claude "))
        .or_else(|| cleaned.strip_prefix("tell cursor "))
        .or_else(|| cleaned.strip_prefix("tell codex "))
    {
        let prompt_text = rest.trim();
        if !prompt_text.is_empty() {
            return Some(VoiceAction::PromptAndSend(prompt_text.to_string()));
        }
    }

    // 3. Open Website: "open website <url>", "open url <url>", "go to <url>"
    if let Some(rest) = cleaned.strip_prefix("open website ")
        .or_else(|| cleaned.strip_prefix("open site "))
        .or_else(|| cleaned.strip_prefix("open url "))
        .or_else(|| cleaned.strip_prefix("go to "))
    {
        let url = rest.trim();
        if !url.is_empty() {
            return Some(VoiceAction::OpenUrl(url.to_string()));
        }
    }

    // 4. Web Search: "search for <query>", "google <query>", "search <query>"
    if let Some(rest) = cleaned.strip_prefix("search for ")
        .or_else(|| cleaned.strip_prefix("google "))
        .or_else(|| cleaned.strip_prefix("search "))
    {
        let query = rest.trim();
        if !query.is_empty() {
            return Some(VoiceAction::WebSearch(query.to_string()));
        }
    }

    // 5. App Switching: "switch to <app>", "open <app>", "focus <app>"
    if let Some(rest) = cleaned.strip_prefix("switch to ")
        .or_else(|| cleaned.strip_prefix("open app "))
        .or_else(|| cleaned.strip_prefix("focus "))
        .or_else(|| cleaned.strip_prefix("open "))
    {
        let target = rest.trim();
        if !target.is_empty() {
            return Some(VoiceAction::SwitchApp(target.to_string()));
        }
    }

    // 6. Prefix commands: "all caps <phrase>", "uppercase <phrase>"
    if let Some(rest) = cleaned.strip_prefix("all caps ")
        .or_else(|| cleaned.strip_prefix("uppercase "))
    {
        let phrase = rest.trim();
        if !phrase.is_empty() {
            return Some(VoiceAction::AllCaps(phrase.to_uppercase()));
        }
    }

    // 7. Prefix commands: "camel case <phrase>"
    if let Some(rest) = cleaned.strip_prefix("camel case ") {
        let phrase = rest.trim();
        if !phrase.is_empty() {
            return Some(VoiceAction::CamelCase(to_camel_case(phrase)));
        }
    }

    // 8. Prefix commands: "snake case <phrase>"
    if let Some(rest) = cleaned.strip_prefix("snake case ") {
        let phrase = rest.trim();
        if !phrase.is_empty() {
            return Some(VoiceAction::SnakeCase(to_snake_case(phrase)));
        }
    }

    // 9. Prefix commands: "title case <phrase>"
    if let Some(rest) = cleaned.strip_prefix("title case ") {
        let phrase = rest.trim();
        if !phrase.is_empty() {
            return Some(VoiceAction::TitleCase(to_title_case(phrase)));
        }
    }

    // 10. Prefix commands: "insert snippet <name>", "paste snippet <name>"
    if let Some(rest) = cleaned.strip_prefix("insert snippet ")
        .or_else(|| cleaned.strip_prefix("paste snippet "))
        .or_else(|| cleaned.strip_prefix("snippet "))
    {
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
    fn parses_undo_and_deletion_actions() {
        assert_eq!(parse_voice_action("scratch that."), Some(VoiceAction::Undo));
        assert_eq!(parse_voice_action("Undo that"), Some(VoiceAction::Undo));
        assert_eq!(parse_voice_action("delete last word"), Some(VoiceAction::DeleteWord));
        assert_eq!(parse_voice_action("delete line!"), Some(VoiceAction::DeleteLine));
        assert_eq!(parse_voice_action("clear all?"), Some(VoiceAction::ClearAll));
    }

    #[test]
    fn parses_formatting_actions() {
        assert_eq!(parse_voice_action("new line."), Some(VoiceAction::NewLine));
        assert_eq!(parse_voice_action("press enter"), Some(VoiceAction::NewLine));
        assert_eq!(parse_voice_action("new paragraph"), Some(VoiceAction::NewParagraph));
        assert_eq!(parse_voice_action("tab"), Some(VoiceAction::Tab));
        assert_eq!(parse_voice_action("bullet point"), Some(VoiceAction::BulletPoint));
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
            Some(VoiceAction::TitleCase("User Authentication Controller".to_string()))
        );
    }

    #[test]
    fn parses_snippet_macros() {
        assert_eq!(
            parse_voice_action("insert snippet email signature"),
            Some(VoiceAction::InsertSnippet("email signature".to_string()))
        );
        assert_eq!(
            parse_voice_action("snippet meeting notes"),
            Some(VoiceAction::InsertSnippet("meeting notes".to_string()))
        );
    }

    #[test]
    fn parses_ai_prompt_and_send() {
        assert_eq!(
            parse_voice_action("prompt write a test for my api"),
            Some(VoiceAction::PromptAndSend("write a test for my api".to_string()))
        );
        assert_eq!(
            parse_voice_action("tell claude refactor this function"),
            Some(VoiceAction::PromptAndSend("refactor this function".to_string()))
        );
        assert_eq!(parse_voice_action("send"), Some(VoiceAction::Submit));
        assert_eq!(parse_voice_action("stop process"), Some(VoiceAction::Interrupt));
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
        assert_eq!(parse_voice_action("scroll down"), Some(VoiceAction::ScrollDown));
    }

    #[test]
    fn parses_search_and_url() {
        assert_eq!(
            parse_voice_action("search for best mechanical keyboard"),
            Some(VoiceAction::WebSearch("best mechanical keyboard".to_string()))
        );
        assert_eq!(
            parse_voice_action("google rust tokio tutorial"),
            Some(VoiceAction::WebSearch("rust tokio tutorial".to_string()))
        );
        assert_eq!(
            parse_voice_action("open website github.com"),
            Some(VoiceAction::OpenUrl("github.com".to_string()))
        );
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
    fn returns_none_for_normal_dictation() {
        assert_eq!(parse_voice_action("Hello world this is a normal sentence."), None);
        assert_eq!(parse_voice_action("I want to buy a new line of shoes"), None);
    }
}
