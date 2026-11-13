use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;
use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;

static FENCED_CODE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)```.*?```").expect("valid regex"));

static SPOILER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)\|\|.*?\|\|").expect("valid regex"));

static MD_LINK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[([^\]]+)\]\([^\)]+\)").expect("valid regex"));

static RAW_URL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"https?://\S+").expect("valid regex"));

static USER_MENTION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<@!?(\d+)>").expect("valid regex"));

static ROLE_MENTION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<@&(\d+)>").expect("valid regex"));

static CHANNEL_MENTION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<#(\d+)>").expect("valid regex"));

static CUSTOM_EMOJI_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<a?:[a-zA-Z0-9_~]+:\d+>").expect("valid regex"));

static INLINE_CODE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"`([^`]+)`").expect("valid regex"));

static MD_FORMATTING_CHARS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[*_~#>]+").expect("valid regex"));

static WHITESPACE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").expect("valid regex"));

pub struct NormalizationContext<'a> {
    pub user_mentions: &'a HashMap<u64, String>,
    pub role_mentions: &'a HashMap<u64, String>,
    pub channel_mentions: &'a HashMap<u64, String>,
    pub max_chars: usize,
    pub max_bytes: usize,
}

impl<'a> Default for NormalizationContext<'a> {
    fn default() -> Self {
        static EMPTY_MAP: LazyLock<HashMap<u64, String>> = LazyLock::new(HashMap::new);
        Self {
            user_mentions: &EMPTY_MAP,
            role_mentions: &EMPTY_MAP,
            channel_mentions: &EMPTY_MAP,
            max_chars: 400,
            max_bytes: 4096,
        }
    }
}

pub fn normalize_message_text(raw_text: &str, ctx: &NormalizationContext<'_>) -> Option<String> {
    if raw_text.trim().is_empty() {
        return None;
    }

    // Step 2: Bound initial string to prevent unbounded regex execution
    let bounded_raw = if raw_text.len() > 8192 {
        &raw_text[..raw_text.floor_char_boundary(8192)]
    } else {
        raw_text
    };

    // Step 3: Strip fenced code blocks and spoiler spans
    let without_code = FENCED_CODE_RE.replace_all(bounded_raw, " ");
    let without_spoilers = SPOILER_RE.replace_all(&without_code, " ");

    // Step 4: Preserve inline-code text while removing backticks
    let inline_code_cleaned = INLINE_CODE_RE.replace_all(&without_spoilers, "$1");

    // Step 6: Retain visible labels of Markdown links [label](url) -> label
    let without_md_links = MD_LINK_RE.replace_all(&inline_code_cleaned, "$1");

    // Replace remaining raw URLs with the word 'link'
    let without_urls = RAW_URL_RE.replace_all(&without_md_links, " link ");

    // Step 7: Resolve user mentions
    let user_resolved = USER_MENTION_RE.replace_all(&without_urls, |caps: &regex::Captures| {
        let id_str = &caps[1];
        if let Ok(id) = id_str.parse::<u64>() {
            if let Some(name) = ctx.user_mentions.get(&id) {
                return name.clone();
            }
        }
        "someone".to_string()
    });

    // Resolve role mentions
    let role_resolved = ROLE_MENTION_RE.replace_all(&user_resolved, |caps: &regex::Captures| {
        let id_str = &caps[1];
        if let Ok(id) = id_str.parse::<u64>() {
            if let Some(name) = ctx.role_mentions.get(&id) {
                return name.clone();
            }
        }
        "a role".to_string()
    });

    // Step 8: Convert channel mentions
    let channel_resolved =
        CHANNEL_MENTION_RE.replace_all(&role_resolved, |caps: &regex::Captures| {
            let id_str = &caps[1];
            if let Ok(id) = id_str.parse::<u64>() {
                if let Some(name) = ctx.channel_mentions.get(&id) {
                    return name.clone();
                }
            }
            "a channel".to_string()
        });

    // Step 9: Replace mass mentions
    let without_mass_mentions = channel_resolved
        .replace("@everyone", "everyone")
        .replace("@here", "here");

    // Step 10: Remove custom emoji markup
    let without_custom_emojis = CUSTOM_EMOJI_RE.replace_all(&without_mass_mentions, " ");

    // Step 5: Remove Markdown formatting symbols while preserving words
    let without_md_formatting = MD_FORMATTING_CHARS_RE.replace_all(&without_custom_emojis, " ");

    // Step 11: Remove disallowed control characters, preserving Unicode for Hindi and whitespace
    let normalized_chars: String = without_md_formatting
        .nfc()
        .filter(|&c| {
            if c.is_control() {
                c == '\n' || c == '\t' || c == ' '
            } else {
                true
            }
        })
        .collect();

    // Step 12: Collapse whitespace
    let collapsed = WHITESPACE_RE.replace_all(&normalized_chars, " ");
    let trimmed = collapsed.trim();

    if trimmed.is_empty() {
        return None;
    }

    // Check if there is at least one alphanumeric character (Latin, Devanagari, digits, etc.)
    let has_alphanumeric = trimmed.chars().any(|c| c.is_alphanumeric());
    if !has_alphanumeric {
        return None;
    }

    // Apply output size limits: max 400 Unicode scalar values and 4 KiB UTF-8
    let max_chars = ctx.max_chars;
    let max_bytes = ctx.max_bytes;

    let char_count = trimmed.chars().count();
    let byte_count = trimmed.len();

    let final_text = if char_count > max_chars || byte_count > max_bytes {
        const SUFFIX: &str = " … message shortened.";
        let suffix_chars = SUFFIX.chars().count();
        let suffix_bytes = SUFFIX.len();

        let target_chars = max_chars.saturating_sub(suffix_chars);
        let target_bytes = max_bytes.saturating_sub(suffix_bytes);

        // Find a safe word boundary
        let mut cur_chars = 0;
        let mut cur_bytes = 0;
        let mut last_safe_boundary = 0;

        for word in trimmed.split_word_bounds() {
            let w_chars = word.chars().count();
            let w_bytes = word.len();

            if cur_chars + w_chars > target_chars || cur_bytes + w_bytes > target_bytes {
                break;
            }

            cur_chars += w_chars;
            cur_bytes += w_bytes;
            last_safe_boundary = cur_bytes;
        }

        if last_safe_boundary == 0 {
            // If even a single word didn't fit, truncate at grapheme boundary
            let mut g_bytes = 0;
            let mut g_chars = 0;
            for g in trimmed.graphemes(true) {
                let gc = g.chars().count();
                let gb = g.len();
                if g_chars + gc > target_chars || g_bytes + gb > target_bytes {
                    break;
                }
                g_chars += gc;
                g_bytes += gb;
            }
            let truncated = &trimmed[..g_bytes];
            format!("{truncated}{SUFFIX}")
        } else {
            let truncated = trimmed[..last_safe_boundary].trim_end();
            format!("{truncated}{SUFFIX}")
        }
    } else {
        trimmed.to_string()
    };

    Some(final_text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plain_english() {
        let ctx = NormalizationContext::default();
        let res = normalize_message_text("hello", &ctx);
        assert_eq!(res.as_deref(), Some("hello"));
    }

    #[test]
    fn test_devanagari_hindi() {
        let ctx = NormalizationContext::default();
        let res = normalize_message_text("भाई, कल शाम को खेलेंगे।", &ctx);
        assert_eq!(res.as_deref(), Some("भाई, कल शाम को खेलेंगे।"));
    }

    #[test]
    fn test_hinglish_mixed() {
        let ctx = NormalizationContext::default();
        let res = normalize_message_text("Bhai kal evening mein game khelenge?", &ctx);
        assert_eq!(res.as_deref(), Some("Bhai kal evening mein game khelenge?"));
    }

    #[test]
    fn test_mention_resolved() {
        let mut user_map = HashMap::new();
        user_map.insert(12345, "Sanket".to_string());
        let ctx = NormalizationContext {
            user_mentions: &user_map,
            ..Default::default()
        };
        let res = normalize_message_text("**hello** <@12345>", &ctx);
        assert_eq!(res.as_deref(), Some("hello Sanket"));
    }

    #[test]
    fn test_mention_unresolved() {
        let ctx = NormalizationContext::default();
        let res = normalize_message_text("Hey <@99999> check this", &ctx);
        assert_eq!(res.as_deref(), Some("Hey someone check this"));
    }

    #[test]
    fn test_urls_and_markdown_links() {
        let ctx = NormalizationContext::default();
        let res = normalize_message_text("Check https://example.com please", &ctx);
        assert_eq!(res.as_deref(), Some("Check link please"));

        let res2 = normalize_message_text("Read [this guide](https://example.com)", &ctx);
        assert_eq!(res2.as_deref(), Some("Read this guide"));
    }

    #[test]
    fn test_code_blocks() {
        let ctx = NormalizationContext::default();
        let res = normalize_message_text("```rust\nfn main() {}\n```", &ctx);
        assert_eq!(res, None);

        let res2 = normalize_message_text(
            "Look at this code: ```python\nprint(1)\n``` pretty cool",
            &ctx,
        );
        assert_eq!(res2.as_deref(), Some("Look at this code: pretty cool"));

        let res3 = normalize_message_text("Run `cargo check` now", &ctx);
        assert_eq!(res3.as_deref(), Some("Run cargo check now"));
    }

    #[test]
    fn test_custom_emojis_and_emojis() {
        let ctx = NormalizationContext::default();
        let res = normalize_message_text("<:pepe_happy:123456789> Good morning!", &ctx);
        assert_eq!(res.as_deref(), Some("Good morning!"));

        let res_emoji_only = normalize_message_text("<:pepe_happy:123456789>", &ctx);
        assert_eq!(res_emoji_only, None);
    }

    #[test]
    fn test_truncation() {
        let ctx = NormalizationContext {
            max_chars: 50,
            max_bytes: 200,
            ..Default::default()
        };
        let long_msg = "This is a very long sentence designed to test the word boundary truncation mechanism properly.";
        let res = normalize_message_text(long_msg, &ctx).unwrap();
        assert!(res.contains("… message shortened."));
        assert!(res.chars().count() <= 50);
    }
}
