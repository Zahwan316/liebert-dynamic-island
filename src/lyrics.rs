use std::collections::{HashMap, VecDeque};
use std::hash::Hash;
use std::sync::{Arc, mpsc};
use std::time::Duration;

use serde::Deserialize;
use ureq::Agent;

use crate::platform::PlatformEvent;

pub const TRANSLATE_LANGUAGES: &[(&str, &str)] = &[
    ("id", "Indonesian"),
    ("en", "English"),
    ("ja", "Japanese"),
    ("ko", "Korean"),
    ("zh-CN", "Chinese"),
    ("ar", "Arabic"),
    ("es", "Spanish"),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LyricWord {
    pub time_ms: u64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LyricLine {
    pub time_ms: u64,
    pub end_ms: Option<u64>,
    pub text: String,
    pub words: Vec<LyricWord>,
    pub romaji: Option<String>,
    pub translation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlainLyricLine {
    pub text: String,
    pub romaji: Option<String>,
    pub translation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LyricsData {
    Loading,
    Synced(Arc<[LyricLine]>),
    Plain(Arc<[PlainLyricLine]>),
    NotFound,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LyricsRequest {
    pub artist: String,
    pub title: String,
    pub duration_secs: Option<u64>,
    pub translate_lang: Option<String>,
    pub need_romaji: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActiveKaraoke<'a> {
    pub text: &'a str,
    pub progress: f32,
    pub next_text: Option<&'a str>,
    pub romaji: Option<&'a str>,
    pub translation: Option<&'a str>,
    pub next_romaji: Option<&'a str>,
    pub next_translation: Option<&'a str>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LrcLibResponse {
    duration: Option<f64>,
    synced_lyrics: Option<String>,
    plain_lyrics: Option<String>,
}

pub fn parse_lrc(lrc: &str) -> Vec<LyricLine> {
    type RawEntry = (u64, Option<(String, Vec<LyricWord>)>);
    let mut entries: Vec<RawEntry> = Vec::new();
    for raw_line in lrc.lines() {
        let mut rest = raw_line.trim();
        let mut timestamps = Vec::new();
        while let Some(stripped) = rest.strip_prefix('[') {
            let Some(end_idx) = stripped.find(']') else {
                break;
            };
            let tag = &stripped[..end_idx];
            if let Some(ms) = parse_lrc_timestamp(tag) {
                timestamps.push(ms);
            }
            rest = stripped[end_idx + 1..].trim_start();
        }
        if timestamps.is_empty() {
            continue;
        }
        let parsed = parse_inline_words(rest);
        let payload = if parsed.0.is_empty() {
            None
        } else {
            Some(parsed)
        };
        for time_ms in timestamps {
            entries.push((time_ms, payload.clone()));
        }
    }
    entries.sort_by_key(|(time_ms, _)| *time_ms);

    let mut out: Vec<LyricLine> = Vec::new();
    for (time_ms, payload) in entries {
        if let Some((text, words)) = payload {
            out.push(LyricLine {
                time_ms,
                end_ms: None,
                text,
                words,
                romaji: None,
                translation: None,
            });
        } else if let Some(prev) = out.last_mut()
            && prev.end_ms.is_none()
            && time_ms > prev.time_ms
        {
            prev.end_ms = Some(time_ms);
        }
    }
    out
}

fn parse_inline_words(raw: &str) -> (String, Vec<LyricWord>) {
    let mut clean = String::with_capacity(raw.len());
    let mut words = Vec::new();
    let mut current_time: Option<u64> = None;
    let mut current_seg = String::new();
    let mut rest = raw.trim();

    while let Some(open) = rest.find('<') {
        let before = &rest[..open];
        if !before.is_empty() {
            clean.push_str(before);
            current_seg.push_str(before);
        }
        let after_open = &rest[open + 1..];
        let Some(close) = after_open.find('>') else {
            clean.push_str(&rest[open..]);
            current_seg.push_str(&rest[open..]);
            rest = "";
            break;
        };
        let tag = &after_open[..close];
        if let Some(ms) = parse_lrc_timestamp(tag) {
            if let Some(t) = current_time.take()
                && !current_seg.trim().is_empty()
            {
                words.push(LyricWord {
                    time_ms: t,
                    text: current_seg.clone(),
                });
            }
            current_seg.clear();
            current_time = Some(ms);
        } else {
            let literal = &rest[open..=open + 1 + close];
            clean.push_str(literal);
            current_seg.push_str(literal);
        }
        rest = &after_open[close + 1..];
    }

    if !rest.is_empty() {
        clean.push_str(rest);
        current_seg.push_str(rest);
    }
    if let Some(t) = current_time
        && !current_seg.trim().is_empty()
    {
        words.push(LyricWord {
            time_ms: t,
            text: current_seg,
        });
    }

    (clean.trim().to_owned(), words)
}

fn parse_lrc_timestamp(tag: &str) -> Option<u64> {
    let (mins_str, sec_part) = tag.split_once(':')?;
    let mins: u64 = mins_str.trim().parse().ok()?;
    let (secs_str, frac_str) = match sec_part.split_once('.') {
        Some((s, f)) => (s, f),
        None => (sec_part, "0"),
    };
    let secs: u64 = secs_str.trim().parse().ok()?;
    let frac_digits = frac_str.trim();
    let ms: u64 = match frac_digits.len() {
        0 => 0,
        1 => frac_digits.parse::<u64>().ok()? * 100,
        2 => frac_digits.parse::<u64>().ok()? * 10,
        _ => frac_digits[..3].parse::<u64>().ok()?,
    };
    Some((mins * 60 + secs) * 1000 + ms)
}

pub fn active_karaoke(data: &LyricsData, position_ms: u64) -> Option<ActiveKaraoke<'_>> {
    match data {
        LyricsData::Synced(lines) if !lines.is_empty() => {
            let next_idx = lines.partition_point(|l| l.time_ms <= position_ms);
            if next_idx == 0 {
                let first = &lines[0];
                let next_line = lines.get(1);
                return Some(ActiveKaraoke {
                    text: first.text.as_str(),
                    progress: 0.0,
                    next_text: next_line.map(|l| l.text.as_str()),
                    romaji: first.romaji.as_deref(),
                    translation: first.translation.as_deref(),
                    next_romaji: next_line.and_then(|l| l.romaji.as_deref()),
                    next_translation: next_line.and_then(|l| l.translation.as_deref()),
                });
            }
            let curr = &lines[next_idx - 1];
            let next_line = lines.get(next_idx);
            let next_ms = next_line
                .map(|l| l.time_ms)
                .unwrap_or_else(|| curr.time_ms.saturating_add(4000));
            let effective_end_ms = match curr.end_ms {
                Some(end) if end > curr.time_ms => end.min(next_ms),
                _ => {
                    if next_ms.saturating_sub(curr.time_ms) > 10_000 {
                        curr.time_ms.saturating_add(7000)
                    } else {
                        next_ms
                    }
                }
            };
            let progress = compute_line_progress(curr, position_ms, effective_end_ms);
            Some(ActiveKaraoke {
                text: curr.text.as_str(),
                progress,
                next_text: next_line.map(|l| l.text.as_str()),
                romaji: curr.romaji.as_deref(),
                translation: curr.translation.as_deref(),
                next_romaji: next_line.and_then(|l| l.romaji.as_deref()),
                next_translation: next_line.and_then(|l| l.translation.as_deref()),
            })
        }
        LyricsData::Plain(lines) => lines.first().map(|first| {
            let next_line = lines.get(1);
            ActiveKaraoke {
                text: first.text.as_str(),
                progress: 1.0,
                next_text: next_line.map(|l| l.text.as_str()),
                romaji: first.romaji.as_deref(),
                translation: first.translation.as_deref(),
                next_romaji: next_line.and_then(|l| l.romaji.as_deref()),
                next_translation: next_line.and_then(|l| l.translation.as_deref()),
            }
        }),
        _ => None,
    }
}

fn compute_line_progress(curr: &LyricLine, pos_ms: u64, next_line_ms: u64) -> f32 {
    if curr.words.len() >= 2 {
        let total_chars: usize = curr
            .words
            .iter()
            .map(|w| w.text.chars().count().max(1))
            .sum();
        let mut passed_chars = 0usize;
        for (i, w) in curr.words.iter().enumerate() {
            let w_chars = w.text.chars().count().max(1);
            let w_end = curr
                .words
                .get(i + 1)
                .map(|n| n.time_ms)
                .unwrap_or_else(|| next_line_ms.max(w.time_ms + 800));
            if pos_ms >= w_end {
                passed_chars += w_chars;
            } else if pos_ms <= w.time_ms {
                return (passed_chars as f32 / total_chars.max(1) as f32).clamp(0.0, 1.0);
            } else {
                let span = (w_end - w.time_ms).max(1) as f32;
                let frac = (pos_ms - w.time_ms) as f32 / span;
                let exact = passed_chars as f32 + (w_chars as f32 * frac);
                return (exact / total_chars.max(1) as f32).clamp(0.0, 1.0);
            }
        }
        return 1.0;
    }

    let span = next_line_ms.saturating_sub(curr.time_ms).max(400) as f32;
    (pos_ms.saturating_sub(curr.time_ms) as f32 / span).clamp(0.0, 1.0)
}

pub fn has_non_latin_script(text: &str) -> bool {
    text.chars().any(|c| c.is_alphabetic() && c > '\u{024F}')
}

#[cfg(test)]
fn has_cjk_or_hangul(text: &str) -> bool {
    text.chars().any(|c| {
        matches!(
            c as u32,
            0x3040..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xAC00..=0xD7AF
        )
    })
}

pub fn has_japanese_kana(text: &str) -> bool {
    text.chars()
        .any(|c| matches!(c as u32, 0x3040..=0x30FF))
}

pub fn romanize_line(text: &str, prefer_japanese: bool) -> Option<String> {
    if !has_non_latin_script(text) {
        return None;
    }
    let raw = if prefer_japanese || has_japanese_kana(text) {
        kakasi::convert(text).romaji
    } else {
        any_ascii::any_ascii(text)
    };
    let normalized = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = normalized.trim();
    if trimmed.is_empty() || trimmed == text.trim() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

pub fn spawn_lyrics_worker(
    req_rx: mpsc::Receiver<LyricsRequest>,
    event_tx: mpsc::Sender<PlatformEvent>,
    on_wake: Arc<dyn Fn() + Send + Sync>,
) {
    std::thread::Builder::new()
        .name("lyrics-worker".to_owned())
        .spawn(move || {
            let config = Agent::config_builder()
                .timeout_global(Some(Duration::from_secs(5)))
                .build();
            let agent: Agent = config.into();
            const MAX_LYRICS_CACHE: usize = 8;
            type EnrichedCacheKey = (String, String, Option<u64>, Option<String>, bool);
            let mut raw_cache: HashMap<(String, String, Option<u64>), LyricsData> = HashMap::new();
            let mut raw_order: VecDeque<(String, String, Option<u64>)> = VecDeque::new();
            let mut enriched_cache: HashMap<EnrichedCacheKey, LyricsData> = HashMap::new();
            let mut enriched_order: VecDeque<EnrichedCacheKey> = VecDeque::new();

            fn cache_insert<K: Eq + Hash + Clone>(
                map: &mut HashMap<K, LyricsData>,
                order: &mut VecDeque<K>,
                key: K,
                val: LyricsData,
                cap: usize,
            ) {
                if !map.contains_key(&key) {
                    while map.len() >= cap {
                        if let Some(old) = order.pop_front() {
                            map.remove(&old);
                        } else {
                            break;
                        }
                    }
                    order.push_back(key.clone());
                }
                map.insert(key, val);
            }

            while let Ok(mut req) = req_rx.recv() {
                while let Ok(newer) = req_rx.try_recv() {
                    req = newer;
                }

                let track_key = (req.artist.clone(), req.title.clone(), req.duration_secs);
                let full_key = (
                    req.artist.clone(),
                    req.title.clone(),
                    req.duration_secs,
                    req.translate_lang.clone(),
                    req.need_romaji,
                );

                let lyrics = if let Some(cached) = enriched_cache.get(&full_key) {
                    cached.clone()
                } else {
                    let base = if let Some(raw) = raw_cache.get(&track_key) {
                        raw.clone()
                    } else {
                        let fetched =
                            fetch_lyrics(&agent, &req.artist, &req.title, req.duration_secs);
                        if !matches!(fetched, LyricsData::NotFound) {
                            cache_insert(
                                &mut raw_cache,
                                &mut raw_order,
                                track_key,
                                fetched.clone(),
                                MAX_LYRICS_CACHE,
                            );
                        }
                        fetched
                    };
                    let enriched = enrich_lyrics_data(
                        &agent,
                        base,
                        req.translate_lang.as_deref(),
                        req.need_romaji,
                    );
                    if !matches!(enriched, LyricsData::NotFound) {
                        cache_insert(
                            &mut enriched_cache,
                            &mut enriched_order,
                            full_key,
                            enriched.clone(),
                            MAX_LYRICS_CACHE,
                        );
                    }
                    enriched
                };

                let _ = event_tx.send(PlatformEvent::LyricsLoaded {
                    artist: req.artist,
                    title: req.title,
                    lyrics,
                });
                on_wake();
            }
        })
        .expect("spawn lyrics worker thread");
}

fn enrich_lyrics_data(
    agent: &Agent,
    data: LyricsData,
    translate_lang: Option<&str>,
    need_romaji: bool,
) -> LyricsData {
    if translate_lang.is_none() && !need_romaji {
        return data;
    }

    match data {
        LyricsData::Synced(lines_arc) => {
            let mut lines = lines_arc.to_vec();
            let texts: Vec<String> = lines.iter().map(|l| l.text.clone()).collect();
            let (romajis, translations) = enrich_lines(agent, &texts, translate_lang, need_romaji);
            for (line, (r, t)) in lines
                .iter_mut()
                .zip(romajis.into_iter().zip(translations))
            {
                line.romaji = r;
                line.translation = t;
            }
            LyricsData::Synced(Arc::from(lines))
        }
        LyricsData::Plain(lines_arc) => {
            let mut lines = lines_arc.to_vec();
            let texts: Vec<String> = lines.iter().map(|l| l.text.clone()).collect();
            let (romajis, translations) = enrich_lines(agent, &texts, translate_lang, need_romaji);
            for (line, (r, t)) in lines
                .iter_mut()
                .zip(romajis.into_iter().zip(translations))
            {
                line.romaji = r;
                line.translation = t;
            }
            LyricsData::Plain(Arc::from(lines))
        }
        other => other,
    }
}

fn enrich_lines(
    agent: &Agent,
    lines: &[String],
    translate_lang: Option<&str>,
    need_romaji: bool,
) -> (Vec<Option<String>>, Vec<Option<String>>) {
    let mut romajis = vec![None; lines.len()];
    let mut translations = vec![None; lines.len()];
    if lines.is_empty() {
        return (romajis, translations);
    }

    if need_romaji {
        let is_japanese_song = lines.iter().any(|l| has_japanese_kana(l));
        for (i, orig) in lines.iter().enumerate() {
            romajis[i] = romanize_line(orig, is_japanese_song);
        }
    }

    if let Some(tl) = translate_lang {
        for (chunk_idx, chunk) in lines.chunks(20).enumerate() {
            let base_idx = chunk_idx * 20;
            let query = chunk.join("\n");
            let (tr_block, _) = query_google_translate(agent, &query, tl).unwrap_or((None, None));
            if let Some(tr) = tr_block {
                for (offset, line_tr) in tr.lines().enumerate() {
                    if let Some(orig) = chunk.get(offset) {
                        let trimmed = line_tr.trim();
                        if !trimmed.is_empty() && !trimmed.eq_ignore_ascii_case(orig.trim()) {
                            translations[base_idx + offset] = Some(trimmed.to_owned());
                        }
                    }
                }
            }
        }
    }

    (romajis, translations)
}

fn query_google_translate(
    agent: &Agent,
    text: &str,
    target_lang: &str,
) -> Option<(Option<String>, Option<String>)> {
    let body = agent
        .get("https://translate.googleapis.com/translate_a/single")
        .header("User-Agent", "LiebertDynamicIsland/0.1")
        .query("client", "gtx")
        .query("sl", "auto")
        .query("tl", target_lang)
        .query("dt", "t")
        .query("q", text)
        .call()
        .ok()?
        .body_mut()
        .with_config()
        .limit(512 * 1024)
        .read_to_string()
        .ok()?;
    Some(parse_google_translate_json(&body))
}

pub fn parse_google_translate_json(body: &str) -> (Option<String>, Option<String>) {
    let Ok(val) = serde_json::from_str::<serde_json::Value>(body) else {
        return (None, None);
    };
    let Some(sentences) = val.get(0).and_then(|v| v.as_array()) else {
        return (None, None);
    };

    let mut translated = String::new();
    let mut romaji = String::new();
    for seg in sentences {
        let Some(arr) = seg.as_array() else {
            continue;
        };
        if let Some(t) = arr.first().and_then(|v| v.as_str()) {
            translated.push_str(t);
        }
        // In translate_a/single dt=rm, source transliteration sits at index 3 (or 2 if target translit).
        if let Some(rm) = arr.get(3).and_then(|v| v.as_str()) {
            if !romaji.is_empty() && !romaji.ends_with('\n') && !rm.starts_with('\n') {
                romaji.push(' ');
            }
            romaji.push_str(rm);
        }
    }

    let tr_out = (!translated.trim().is_empty()).then_some(translated);
    let rm_out = (!romaji.trim().is_empty()).then_some(romaji);
    (tr_out, rm_out)
}

fn fetch_lyrics(
    agent: &Agent,
    artist: &str,
    title: &str,
    duration_secs: Option<u64>,
) -> LyricsData {
    let clean_title = strip_title_noise(title);
    if let Some(data) = query_lrclib_get(agent, artist, &clean_title, duration_secs) {
        return data;
    }
    query_lrclib_search(agent, artist, &clean_title, duration_secs)
        .unwrap_or(LyricsData::NotFound)
}

fn query_lrclib_get(
    agent: &Agent,
    artist: &str,
    title: &str,
    duration_secs: Option<u64>,
) -> Option<LyricsData> {
    if artist.trim().is_empty() || title.trim().is_empty() {
        return None;
    }
    if let Some(dur) = duration_secs.filter(|&d| d > 0)
        && let Some(data) = call_lrclib_get(agent, artist, title, Some(dur))
    {
        return Some(data);
    }
    call_lrclib_get(agent, artist, title, None)
}

fn call_lrclib_get(
    agent: &Agent,
    artist: &str,
    title: &str,
    duration_secs: Option<u64>,
) -> Option<LyricsData> {
    let mut req = agent
        .get("https://lrclib.net/api/get")
        .header("User-Agent", "LiebertDynamicIsland/0.1")
        .query("artist_name", artist)
        .query("track_name", title);
    if let Some(dur) = duration_secs {
        req = req.query("duration", dur.to_string());
    }
    let body = req
        .call()
        .ok()?
        .body_mut()
        .with_config()
        .limit(512 * 1024)
        .read_to_string()
        .ok()?;
    let parsed: LrcLibResponse = serde_json::from_str(&body).ok()?;
    response_to_lyrics(parsed)
}

fn query_lrclib_search(
    agent: &Agent,
    artist: &str,
    title: &str,
    duration_secs: Option<u64>,
) -> Option<LyricsData> {
    let q = format!("{artist} {title}").trim().to_owned();
    if q.is_empty() {
        return None;
    }
    let body = agent
        .get("https://lrclib.net/api/search")
        .header("User-Agent", "LiebertDynamicIsland/0.1")
        .query("q", &q)
        .call()
        .ok()?
        .body_mut()
        .with_config()
        .limit(512 * 1024)
        .read_to_string()
        .ok()?;
    let mut list: Vec<LrcLibResponse> = serde_json::from_str(&body).ok()?;
    if let Some(target) = duration_secs.filter(|&d| d > 0) {
        let target_f = target as f64;
        list.sort_by(|a, b| {
            let a_synced = a.synced_lyrics.as_ref().is_some_and(|s| !s.trim().is_empty());
            let b_synced = b.synced_lyrics.as_ref().is_some_and(|s| !s.trim().is_empty());
            let a_diff = a.duration.map(|d| (d - target_f).abs()).unwrap_or(f64::MAX);
            let b_diff = b.duration.map(|d| (d - target_f).abs()).unwrap_or(f64::MAX);
            b_synced.cmp(&a_synced).then_with(|| a_diff.total_cmp(&b_diff))
        });
    }
    list.into_iter().find_map(response_to_lyrics)
}

fn response_to_lyrics(resp: LrcLibResponse) -> Option<LyricsData> {
    if let Some(synced) = resp.synced_lyrics {
        let lines = parse_lrc(&synced);
        if !lines.is_empty() {
            return Some(LyricsData::Synced(Arc::from(lines)));
        }
    }
    if let Some(plain) = resp.plain_lyrics {
        let lines: Vec<PlainLyricLine> = plain
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(|text| PlainLyricLine {
                text: text.to_owned(),
                romaji: None,
                translation: None,
            })
            .collect();
        if !lines.is_empty() {
            return Some(LyricsData::Plain(Arc::from(lines)));
        }
    }
    None
}

fn strip_title_noise(title: &str) -> String {
    let mut s = title.trim().to_owned();
    for marker in [" (Official", " [Official", " - Remastered", " (Remastered"] {
        if let Some(idx) = s.find(marker) {
            s.truncate(idx);
        }
    }
    s.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_lrc_handles_multi_tags_and_sorts() {
        let lrc = "\
[ar:Artist]
[00:12.50][00:30.000]Chorus line
[00:05.2]Intro line
[00:08.00]
";
        let lines = parse_lrc(lrc);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].time_ms, 5200);
        assert_eq!(lines[0].text, "Intro line");
        assert_eq!(lines[1].time_ms, 12500);
        assert_eq!(lines[2].time_ms, 30000);
    }

    #[test]
    fn parse_lrc_parses_enhanced_word_timestamps() {
        let lrc = "[00:10.00]<00:10.00>Hello <00:10.50>karaoke <00:11.00>world";
        let lines = parse_lrc(lrc);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "Hello karaoke world");
        assert_eq!(lines[0].words.len(), 3);
        assert_eq!(lines[0].words[1].time_ms, 10500);
        assert_eq!(lines[0].words[1].text.trim(), "karaoke");
    }

    #[test]
    fn active_karaoke_computes_line_and_word_progress() {
        let lines = parse_lrc(
            "[00:00.00]<00:00.00>La <00:01.00>la\n[00:02.00]Next line",
        );
        let data = LyricsData::Synced(Arc::from(lines));
        let mid = active_karaoke(&data, 1000).expect("active line");
        assert_eq!(mid.text, "La la");
        assert!(mid.progress > 0.3 && mid.progress < 0.8);
        assert_eq!(mid.next_text, Some("Next line"));

        let next = active_karaoke(&data, 4000).expect("next line");
        assert_eq!(next.text, "Next line");
        assert!(next.progress > 0.4 && next.progress < 0.6);
    }

    #[test]
    fn romanize_handles_japanese_kanji_chinese_korean_arabic_and_skips_latin() {
        assert!(!has_non_latin_script("Hello World"));
        assert!(has_non_latin_script("こんにちは世界"));
        assert!(has_non_latin_script("مرحبا"));
        assert!(has_cjk_or_hangul("さくら"));
        assert_eq!(romanize_line("Hello World", false), None);

        let jp = romanize_line("この先に出会うどんな友とも", true).expect("japanese romaji");
        assert!(jp.contains("kono") && jp.contains("saki"), "got: {jp}");

        let zh = romanize_line("你好世界", false).expect("chinese romanization");
        assert!(zh.is_ascii() && !zh.is_empty(), "got: {zh}");

        let ko = romanize_line("사랑해요", false).expect("korean romanization");
        assert!(ko.is_ascii() && !ko.is_empty(), "got: {ko}");

        let ar = romanize_line("مرحبا", false).expect("arabic romanization");
        assert!(ar.is_ascii() && !ar.is_empty(), "got: {ar}");
    }

    #[test]
    fn parse_google_translate_json_extracts_translation_and_romaji() {
        let payload = r#"[
            [
                ["Aku mencintaimu\n", "愛してる\n", null, "Aishiteru\n"],
                ["Selamanya", "ずっと", null, "Zutto"]
            ],
            null,
            "ja"
        ]"#;
        let (tr, rm) = parse_google_translate_json(payload);
        assert_eq!(tr.as_deref(), Some("Aku mencintaimu\nSelamanya"));
        assert_eq!(rm.as_deref(), Some("Aishiteru\nZutto"));
    }

    #[test]
    fn parse_lrc_sets_end_ms_on_instrumental_break_and_limits_karaoke_progress() {
        let lrc = "\
[00:10.00]Before solo
[00:14.00]
[00:40.00]After solo
";
        let lines = parse_lrc(lrc);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].end_ms, Some(14_000));
        assert_eq!(lines[1].end_ms, None);

        let data = LyricsData::Synced(Arc::from(lines));
        let at_break = active_karaoke(&data, 15_000).expect("line before solo");
        assert_eq!(at_break.text, "Before solo");
        assert_eq!(at_break.progress, 1.0);
    }
}
