//! Text normalization used for binding keys and external (SMTC) track keys.
//!
//! Rules:
//! - lowercase (Unicode aware), `ё` folded to `е`;
//! - bracketed segments `(...)`/`[...]`/`{...}` that contain a "noise" keyword
//!   (remastered, live, deluxe, feat., ...) are removed;
//! - dash suffixes like ` - 2011 Remaster` or ` - Radio Edit` are removed;
//! - all punctuation is dropped, whitespace collapsed to single spaces.

/// Keywords that mark a bracketed or dash-separated suffix as noise.
const NOISE_KEYWORDS: &[&str] = &[
    "remaster",
    "remastered",
    "remix edit",
    "radio edit",
    "single edit",
    "edit",
    "live",
    "deluxe",
    "bonus track",
    "bonus",
    "explicit",
    "clean",
    "mono",
    "stereo",
    "version",
    "anniversary",
    "expanded",
    "feat",
    "ft",
    "featuring",
    "official video",
    "official audio",
    "lyrics",
];

fn contains_noise_keyword(segment: &str) -> bool {
    let words = alnum_words(segment);
    let joined = words.join(" ");
    NOISE_KEYWORDS.iter().any(|kw| {
        if kw.contains(' ') {
            joined.contains(kw)
        } else {
            words.iter().any(|w| w == kw)
        }
    })
}

/// Split into lowercase alphanumeric words.
fn alnum_words(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for ch in s.chars() {
        if ch.is_alphanumeric() {
            for lc in ch.to_lowercase() {
                cur.push(if lc == 'ё' { 'е' } else { lc });
            }
        } else if !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Remove bracketed segments containing noise keywords.
fn strip_noise_brackets(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let close = match chars[i] {
            '(' => Some(')'),
            '[' => Some(']'),
            '{' => Some('}'),
            _ => None,
        };
        if let Some(close_ch) = close {
            if let Some(rel) = chars[i + 1..].iter().position(|&c| c == close_ch) {
                let inner: String = chars[i + 1..i + 1 + rel].iter().collect();
                if contains_noise_keyword(&inner) {
                    i += rel + 2;
                    continue;
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Remove ` - <noise>` suffixes (e.g. "Song - 2011 Remaster").
fn strip_noise_dash_suffix(s: &str) -> String {
    let mut result = s.to_string();
    for sep in [" - ", " – ", " — "] {
        if let Some(pos) = result.rfind(sep) {
            let suffix = &result[pos + sep.len()..];
            if contains_noise_keyword(suffix) {
                result.truncate(pos);
            }
        }
    }
    result
}

/// Normalize a free-form name (artist, title, album).
pub fn normalize_name(input: &str) -> String {
    let stage1 = strip_noise_brackets(input);
    let stage2 = strip_noise_dash_suffix(&stage1);
    alnum_words(&stage2).join(" ")
}

/// Build key for an external (SMTC) track: `ext:<artist>|<title>`.
pub fn external_track_key(artist: &str, title: &str) -> String {
    format!("ext:{}|{}", normalize_name(artist), normalize_name(title))
}

/// Build binding key for an album: `<artist>|<album>`.
pub fn album_key(artist: &str, album: &str) -> String {
    format!("{}|{}", normalize_name(artist), normalize_name(album))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowercases_and_drops_punctuation() {
        assert_eq!(normalize_name("AC/DC"), "ac dc");
        assert_eq!(normalize_name("  Hello,   World!!  "), "hello world");
        assert_eq!(normalize_name("Don't Stop Me Now"), "don t stop me now");
    }

    #[test]
    fn strips_remaster_suffixes() {
        assert_eq!(
            normalize_name("Bohemian Rhapsody - Remastered 2011"),
            "bohemian rhapsody"
        );
        assert_eq!(
            normalize_name("Bohemian Rhapsody (2011 Remaster)"),
            "bohemian rhapsody"
        );
        assert_eq!(normalize_name("Song [Live]"), "song");
        assert_eq!(normalize_name("Song (feat. Someone)"), "song");
        assert_eq!(normalize_name("Song - Radio Edit"), "song");
    }

    #[test]
    fn keeps_meaningful_brackets_and_dashes() {
        assert_eq!(normalize_name("Song (Part 2)"), "song part 2");
        assert_eq!(normalize_name("Artist - Title"), "artist title");
    }

    #[test]
    fn handles_cyrillic() {
        assert_eq!(normalize_name("Тело похудело"), "тело похудело");
        assert_eq!(normalize_name("КОРЗА"), "корза");
        assert_eq!(normalize_name("Ёлка"), "елка");
    }

    #[test]
    fn external_key_is_stable() {
        let a = external_track_key("Queen", "Bohemian Rhapsody - 2011 Remaster");
        let b = external_track_key("QUEEN", "Bohemian Rhapsody");
        assert_eq!(a, b);
        assert_eq!(a, "ext:queen|bohemian rhapsody");
    }

    #[test]
    fn album_key_format() {
        assert_eq!(
            album_key("Pink Floyd", "The Dark Side of the Moon (Remastered)"),
            "pink floyd|the dark side of the moon"
        );
    }

    #[test]
    fn empty_input() {
        assert_eq!(normalize_name(""), "");
        assert_eq!(external_track_key("", ""), "ext:|");
    }
}
