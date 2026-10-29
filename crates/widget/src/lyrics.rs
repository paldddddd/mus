use std::fs;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};

const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LyricLine {
    pub time_ms: u64,
    pub text: String,
}

/// Strip typical noise from titles and artist tags (e.g. "(feat. ...)", "[Official Video]", "(Slowed + Reverb)")
fn clean_query_part(s: &str) -> String {
    let mut clean = s.to_string();
    // Remove bracketed or parenthesized additions
    while let Some(start) = clean.find('(') {
        if let Some(end) = clean[start..].find(')') {
            let inner = &clean[start + 1..start + end].to_lowercase();
            if inner.contains("feat")
                || inner.contains("ft.")
                || inner.contains("prod")
                || inner.contains("remix")
                || inner.contains("slowed")
                || inner.contains("reverb")
                || inner.contains("acoustic")
                || inner.contains("radio")
                || inner.contains("edit")
                || inner.contains("version")
                || inner.contains("cover")
            {
                clean.replace_range(start..=start + end, "");
                continue;
            }
        }
        break;
    }
    while let Some(start) = clean.find('[') {
        if let Some(end) = clean[start..].find(']') {
            clean.replace_range(start..=start + end, "");
            continue;
        }
        break;
    }
    // Remove "feat. ..." at end
    if let Some(pos) = clean.to_lowercase().find("feat.") {
        clean.truncate(pos);
    }
    if let Some(pos) = clean.to_lowercase().find("ft.") {
        clean.truncate(pos);
    }
    clean.trim().to_string()
}

/// Parse standard `.lrc` timestamped lyrics (`[mm:ss.xx] lyric text`).
pub fn parse_lrc(content: &str) -> Vec<LyricLine> {
    let mut lines = Vec::new();
    for raw_line in content.lines() {
        let trimmed = raw_line.trim();
        if !trimmed.starts_with('[') {
            continue;
        }
        if let Some(close_idx) = trimmed.find(']') {
            let tag = &trimmed[1..close_idx];
            let text = trimmed[close_idx + 1..].trim();
            if text.is_empty() {
                continue;
            }
            if let Some(colon_idx) = tag.find(':') {
                let min_str = &tag[..colon_idx];
                let sec_str = &tag[colon_idx + 1..];
                if let (Ok(mins), Ok(secs)) = (min_str.parse::<u64>(), sec_str.parse::<f64>()) {
                    let time_ms = mins * 60_000 + (secs * 1000.0).round() as u64;
                    lines.push(LyricLine {
                        time_ms,
                        text: text.to_string(),
                    });
                }
            }
        }
    }
    lines.sort_by_key(|l| l.time_ms);
    lines
}

/// Convert plain (unsynced) lyrics into evenly-spaced `LyricLine` entries so every song can display lyrics.
pub fn parse_plain_lyrics(content: &str, duration_ms: u64) -> Vec<LyricLine> {
    let raw: Vec<&str> = content
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();
    if raw.is_empty() {
        return Vec::new();
    }
    let total_ms = if duration_ms > 15_000 { duration_ms - 5_000 } else { 180_000 };
    let step_ms = (total_ms / (raw.len() as u64).max(1)).max(2500);
    raw.into_iter()
        .enumerate()
        .map(|(i, text)| LyricLine {
            time_ms: 2000 + (i as u64) * step_ms,
            text: text.to_string(),
        })
        .collect()
}

/// Non-blocking synced lyrics manager with local disk cache and LRCLIB fallback.
pub struct LyricsManager {
    current_key: String,
    active_key: Arc<Mutex<String>>,
    lines: Arc<Mutex<Vec<LyricLine>>>,
}

impl Default for LyricsManager {
    fn default() -> Self {
        Self::new()
    }
}

impl LyricsManager {
    pub fn new() -> Self {
        Self {
            current_key: String::new(),
            active_key: Arc::new(Mutex::new(String::new())),
            lines: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn cache_file_for(key: &str) -> PathBuf {
        let safe_name: String = key
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' || c == ' ' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let dir = common::app_data_dir().join("lyrics");
        let _ = fs::create_dir_all(&dir);
        dir.join(format!("{}.lrc", safe_name.trim()))
    }

    pub fn on_track_changed(&mut self, title: &str, artist: &str) {
        let clean_title = title.trim();
        let clean_artist = artist.trim();
        if clean_title.is_empty()
            || clean_title == "No media playing"
            || clean_title == "Нет воспроизведения"
        {
            self.current_key.clear();
            if let Ok(mut g) = self.lines.lock() {
                g.clear();
            }
            return;
        }

        let key = format!("{} - {}", clean_artist, clean_title);
        if key == self.current_key {
            return;
        }
        self.current_key = key.clone();
        if let Ok(mut ak) = self.active_key.lock() {
            *ak = key.clone();
        }
        if let Ok(mut g) = self.lines.lock() {
            g.clear();
        }

        let lines_arc = Arc::clone(&self.lines);
        let active_key_arc = Arc::clone(&self.active_key);
        let t_owned = clean_title.to_string();
        let a_owned = clean_artist.to_string();

        std::thread::spawn(move || {
            let cache_path = Self::cache_file_for(&key);
            if cache_path.exists() {
                if let Ok(cached_lrc) = fs::read_to_string(&cache_path) {
                    let parsed = parse_lrc(&cached_lrc);
                    if let Ok(ak) = active_key_arc.lock() {
                        if *ak == key {
                            if let Ok(mut g) = lines_arc.lock() {
                                *g = parsed;
                            }
                        }
                    }
                    return;
                }
            }

            // 1. Query LRCLIB exact match API asynchronously via built-in Windows curl.exe
            let clean_t = clean_query_part(&t_owned);
            let first_artist = a_owned.split(',').next().unwrap_or(&a_owned);
            let clean_a = clean_query_part(first_artist);

            let mut found_lines: Vec<LyricLine> = Vec::new();

            let try_parse_json = |bytes: &[u8]| -> Option<Vec<LyricLine>> {
                if let Ok(json) = serde_json::from_slice::<serde_json::Value>(bytes) {
                    if let Some(synced) = json.get("syncedLyrics").and_then(|v| v.as_str()) {
                        if !synced.trim().is_empty() {
                            return Some(parse_lrc(synced));
                        }
                    }
                    if let Some(plain) = json.get("plainLyrics").and_then(|v| v.as_str()) {
                        if !plain.trim().is_empty() {
                            return Some(parse_plain_lyrics(plain, 180_000));
                        }
                    }
                }
                None
            };

            // Attempt A: Exact match with raw title & artist
            let output_a = Command::new("curl.exe")
                .creation_flags(CREATE_NO_WINDOW)
                .args([
                    "-s", "-m", "4", "--get", "https://lrclib.net/api/get",
                    "--data-urlencode", &format!("track_name={}", t_owned),
                    "--data-urlencode", &format!("artist_name={}", a_owned),
                ])
                .output();

            if let Ok(out) = output_a {
                if out.status.success() {
                    if let Some(lines) = try_parse_json(&out.stdout) {
                        found_lines = lines;
                    }
                }
            }

            // Attempt B: Exact match with cleaned title & clean primary artist
            if found_lines.is_empty() && (!clean_t.is_empty() || !clean_a.is_empty()) {
                let target_t = if clean_t.is_empty() { &t_owned } else { &clean_t };
                let target_a = if clean_a.is_empty() { &a_owned } else { &clean_a };
                let output_b = Command::new("curl.exe")
                    .creation_flags(CREATE_NO_WINDOW)
                    .args([
                        "-s", "-m", "4", "--get", "https://lrclib.net/api/get",
                        "--data-urlencode", &format!("track_name={}", target_t),
                        "--data-urlencode", &format!("artist_name={}", target_a),
                    ])
                    .output();

                if let Ok(out) = output_b {
                    if out.status.success() {
                        if let Some(lines) = try_parse_json(&out.stdout) {
                            found_lines = lines;
                        }
                    }
                }
            }

            // Attempt C: Search query fallback with artist + title
            if found_lines.is_empty() {
                let target_t = if clean_t.is_empty() { &t_owned } else { &clean_t };
                let target_a = if clean_a.is_empty() { &a_owned } else { &clean_a };
                let query = format!("{} {}", target_a, target_t);
                let search_out = Command::new("curl.exe")
                    .creation_flags(CREATE_NO_WINDOW)
                    .args([
                        "-s", "-m", "5", "--get", "https://lrclib.net/api/search",
                        "--data-urlencode", &format!("q={}", query),
                    ])
                    .output();

                if let Ok(out) = search_out {
                    if out.status.success() {
                        if let Ok(arr) = serde_json::from_slice::<Vec<serde_json::Value>>(&out.stdout) {
                            // Prefer syncedLyrics first
                            for item in &arr {
                                if let Some(synced) = item.get("syncedLyrics").and_then(|v| v.as_str()) {
                                    if !synced.trim().is_empty() {
                                        found_lines = parse_lrc(synced);
                                        break;
                                    }
                                }
                            }
                            // Fallback to plainLyrics
                            if found_lines.is_empty() {
                                for item in &arr {
                                    if let Some(plain) = item.get("plainLyrics").and_then(|v| v.as_str()) {
                                        if !plain.trim().is_empty() {
                                            found_lines = parse_plain_lyrics(plain, 180_000);
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Attempt D: Search query fallback with just cleaned title
            if found_lines.is_empty() && !clean_t.is_empty() {
                let search_out = Command::new("curl.exe")
                    .creation_flags(CREATE_NO_WINDOW)
                    .args([
                        "-s", "-m", "5", "--get", "https://lrclib.net/api/search",
                        "--data-urlencode", &format!("q={}", clean_t),
                    ])
                    .output();

                if let Ok(out) = search_out {
                    if out.status.success() {
                        if let Ok(arr) = serde_json::from_slice::<Vec<serde_json::Value>>(&out.stdout) {
                            for item in &arr {
                                if let Some(synced) = item.get("syncedLyrics").and_then(|v| v.as_str()) {
                                    if !synced.trim().is_empty() {
                                        found_lines = parse_lrc(synced);
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if !found_lines.is_empty() {
                // Save to local disk cache
                let mut content = String::new();
                for l in &found_lines {
                    let mins = l.time_ms / 60_000;
                    let secs = (l.time_ms % 60_000) as f64 / 1000.0;
                    content.push_str(&format!("[{:02}:{:05.2}] {}\n", mins, secs, l.text));
                }
                let _ = fs::write(&cache_path, content);
            }

            if !found_lines.is_empty() {
                if let Ok(ak) = active_key_arc.lock() {
                    if *ak == key {
                        if let Ok(mut g) = lines_arc.lock() {
                            *g = found_lines;
                        }
                    }
                }
            }
        });
    }

    /// Return the active lyric line for `position_ms`, if any.
    pub fn current_line(&self, position_ms: u64) -> Option<String> {
        self.current_and_next(position_ms).map(|(cur, _, _)| cur)
    }

    /// Return `(current_line, upcoming_lines)` for `position_ms`.
    /// `upcoming_lines` contains up to `count` lines after the active line.
    pub fn current_and_upcoming(&self, position_ms: u64, count: usize) -> Option<(String, Vec<String>)> {
        let g = self.lines.lock().ok()?;
        if g.is_empty() {
            return None;
        }
        let idx = g.partition_point(|l| l.time_ms <= position_ms);
        let cur_i = if idx == 0 { 0 } else { idx - 1 };
        let cur = g[cur_i].text.clone();
        let mut upcoming = Vec::with_capacity(count);
        for i in (cur_i + 1)..(cur_i + 1 + count) {
            if let Some(l) = g.get(i) {
                upcoming.push(l.text.clone());
            }
        }
        Some((cur, upcoming))
    }

    /// Return `(current_line, next_line, third_line)` for `position_ms`.
    pub fn current_and_next(&self, position_ms: u64) -> Option<(String, Option<String>, Option<String>)> {
        let (cur, upcoming) = self.current_and_upcoming(position_ms, 2)?;
        Some((cur, upcoming.get(0).cloned(), upcoming.get(1).cloned()))
    }
}
