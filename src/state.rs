use std::collections::VecDeque;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// Upper bound for pending notifications so a noisy source cannot grow memory without limit.
pub const MAX_QUEUED_NOTIFICATIONS: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationItem {
    pub id: String,
    pub app_name: String,
    pub title: String,
    pub body: String,
    pub icon_rgba: Option<Vec<u8>>,
    pub timestamp: u64,
}

/// Strips HTML/XML tags (including entity-escaped markup like `&lt;a href="..."&gt;`)
/// and normalizes entities/whitespace from browser and Electron notifications
/// while keeping math (`1 < 2`), hearts (`<3`), and email addresses intact.
pub fn sanitize_notification_text(raw: &str) -> String {
    let mut cur = raw.to_owned();
    for _ in 0..2 {
        let stripped = strip_html_tags(&cur);
        let decoded = decode_html_entities(&stripped);
        let cleaned = strip_html_tags(&decoded);
        if cleaned == cur {
            break;
        }
        cur = cleaned;
    }
    collapse_notification_whitespace(&cur)
}

/// Returns whether `app_name` identifies the Brave browser (desktop/snap/flatpak variants).
pub fn is_brave_app(app_name: &str) -> bool {
    let lower = app_name.trim().to_ascii_lowercase();
    lower == "brave"
        || lower.starts_with("brave ")
        || lower.starts_with("brave-")
        || lower.contains("brave browser")
}

/// Returns whether `text` represents a bare website origin/domain attribution line
/// (such as `"web.whatsapp.com"`, `"via youtube.com"`, or `"https://discord.com"`).
pub fn is_web_origin_line(text: &str) -> bool {
    let mut s = text.trim();
    if s.is_empty() {
        return false;
    }
    let lower_orig = s.to_ascii_lowercase();
    for prefix in ["via ", "from ", "source: "] {
        if lower_orig.starts_with(prefix) {
            s = s[prefix.len()..].trim();
            break;
        }
    }
    if let Some(rest) = s
        .strip_prefix("https://")
        .or_else(|| s.strip_prefix("http://"))
    {
        s = rest;
    }
    if let Some((host, _)) = s.split_once('/') {
        s = host;
    }
    let host = s.split(':').next().unwrap_or(s).trim_end_matches('.');
    if host.is_empty() || host.contains('@') || host.chars().any(char::is_whitespace) {
        return false;
    }
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    let parts: Vec<&str> = host.split('.').collect();
    if parts.len() < 2 {
        return false;
    }
    let all_labels_valid = parts.iter().all(|p| {
        !p.is_empty()
            && p.len() <= 63
            && !p.starts_with('-')
            && !p.ends_with('-')
            && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    });
    if !all_labels_valid {
        return false;
    }
    let tld = parts.last().copied().unwrap_or("");
    let has_alpha_host = parts
        .iter()
        .any(|p| p.chars().any(|c| c.is_ascii_alphabetic()));
    has_alpha_host && tld.len() >= 2 && tld.chars().all(|c| c.is_ascii_alphabetic())
}

fn strip_brave_origin_from_text(input: &str) -> String {
    let mut kept_lines = Vec::new();
    for raw_line in input.lines() {
        let mut line = raw_line.trim().to_owned();
        if line.is_empty() || is_web_origin_line(&line) {
            continue;
        }
        // Strip leading "<domain>: " / "<domain> - " / "<domain> " prefix emitted when Chromium
        // wraps web origin before notification text.
        if let Some((first_tok, rest)) = line.split_once(char::is_whitespace) {
            let clean_tok = first_tok.trim_end_matches([':', '-', '—', '–', '·', '•', '|']);
            if is_web_origin_line(clean_tok) {
                let after = rest
                    .trim_start_matches([':', '-', '—', '–', '·', '•', '|', ' '])
                    .trim();
                if !after.is_empty() {
                    line = after.to_owned();
                } else {
                    continue;
                }
            }
        }
        // Strip trailing " — web.whatsapp.com", " (youtube.com)", etc.
        for sep in [" — ", " – ", " - ", " · ", " • ", " | "] {
            if let Some((head, tail)) = line.rsplit_once(sep)
                && is_web_origin_line(tail)
                && !head.trim().is_empty()
            {
                line = head.trim().to_owned();
                break;
            }
        }
        if let Some(open) = line.rfind(" (")
            && line.ends_with(')')
        {
            let inside = &line[open + 2..line.len() - 1];
            if is_web_origin_line(inside) && !line[..open].trim().is_empty() {
                line = line[..open].trim().to_owned();
            }
        }
        if !is_web_origin_line(&line) && !line.is_empty() {
            kept_lines.push(line);
        }
    }
    kept_lines.join(" ")
}

fn clean_notification_field(raw: &str, brave_mode: bool) -> String {
    let mut cur = raw.to_owned();
    for _ in 0..2 {
        let stripped = strip_html_tags(&cur);
        let decoded = decode_html_entities(&stripped);
        let cleaned = strip_html_tags(&decoded);
        if cleaned == cur {
            break;
        }
        cur = cleaned;
    }
    if brave_mode {
        cur = strip_brave_origin_from_text(&cur);
    }
    collapse_notification_whitespace(&cur)
}

/// Builds a [`NotificationItem`] with sanitized `app_name`, `title`, and `body`.
pub fn sanitized_notification_item(
    id: String,
    app_name: &str,
    default_app: &str,
    title: &str,
    body: &str,
    icon_rgba: Option<Vec<u8>>,
    timestamp: u64,
) -> NotificationItem {
    let clean_app = sanitize_notification_text(app_name);
    let final_app = if clean_app.is_empty() {
        default_app.to_owned()
    } else {
        clean_app
    };
    let brave = is_brave_app(&final_app);
    let clean_title = clean_notification_field(title, brave);
    let final_title = if clean_title.is_empty() {
        final_app.clone()
    } else {
        clean_title
    };
    NotificationItem {
        id,
        app_name: final_app,
        title: final_title,
        body: clean_notification_field(body, brave),
        icon_rgba,
        timestamp,
    }
}

fn is_known_html_tag_prefix(tag_lower: &str) -> bool {
    matches!(
        tag_lower,
        "a" | "abbr"
            | "b"
            | "br"
            | "code"
            | "div"
            | "em"
            | "font"
            | "h1"
            | "h2"
            | "h3"
            | "hr"
            | "i"
            | "img"
            | "li"
            | "ol"
            | "p"
            | "pre"
            | "s"
            | "small"
            | "span"
            | "strong"
            | "sub"
            | "sup"
            | "table"
            | "td"
            | "th"
            | "tr"
            | "u"
            | "ul"
    )
}

fn is_break_tag_name(name_lower: &str, closing: bool) -> bool {
    matches!(name_lower, "br" | "hr")
        || (closing && matches!(name_lower, "p" | "div" | "li" | "tr" | "h1" | "h2" | "h3"))
}

fn strip_html_tags(input: &str) -> String {
    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut out = String::with_capacity(len);
    let mut i = 0;

    while i < len {
        if bytes[i] == b'<' {
            let rest = &input[i..];
            if rest.starts_with("<!--")
                && let Some(end_rel) = rest.find("-->")
            {
                i += end_rel + 3;
                continue;
            }

            let mut cursor = i + 1;
            let closing = if cursor < len && bytes[cursor] == b'/' {
                cursor += 1;
                true
            } else {
                false
            };

            let tag_start = cursor;
            let first = bytes.get(cursor).copied();
            if let Some(ch) = first
                && (ch.is_ascii_alphabetic() || ch == b'!' || ch == b'?')
            {
                cursor += 1;
                while cursor < len
                    && (bytes[cursor].is_ascii_alphanumeric()
                        || bytes[cursor] == b':'
                        || bytes[cursor] == b'-')
                {
                    cursor += 1;
                }
                let tag_name = &input[tag_start..cursor];
                let delim = bytes.get(cursor).copied();
                let valid_delim = matches!(delim, Some(b'>' | b'/' | b' ' | b'\t' | b'\n' | b'\r'));
                if valid_delim || (delim.is_none() && tag_name.is_ascii()) {
                    let mut scan = cursor;
                    let mut quote: Option<u8> = None;
                    let mut found_gt = false;
                    while scan < len && scan - i <= 2048 {
                        let b = bytes[scan];
                        if let Some(q) = quote {
                            if b == q {
                                quote = None;
                            }
                        } else if b == b'"' || b == b'\'' {
                            quote = Some(b);
                        } else if b == b'>' {
                            found_gt = true;
                            scan += 1;
                            break;
                        }
                        scan += 1;
                    }
                    let tag_lower = tag_name.to_ascii_lowercase();
                    if found_gt {
                        if is_break_tag_name(&tag_lower, closing) && !out.ends_with(' ') {
                            out.push(' ');
                        }
                        i = scan;
                        continue;
                    } else if scan == len
                        && is_known_html_tag_prefix(&tag_lower)
                        && matches!(delim, Some(b' ' | b'/' | b'\t') | None)
                    {
                        // Truncated partial tag such as `<a href="https://...` at payload tail.
                        i = len;
                        continue;
                    }
                }
            }
        }

        let ch = input[i..].chars().next().unwrap_or('\0');
        out.push(ch);
        i += ch.len_utf8();
    }

    out
}

fn decode_html_entities(input: &str) -> String {
    if !input.contains('&') {
        return input.to_owned();
    }
    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut out = String::with_capacity(len);
    let mut i = 0;

    while i < len {
        if bytes[i] == b'&' {
            let max_end = (i + 12).min(len);
            if let Some(semi_rel) = input[i + 1..max_end].find(';') {
                let entity = &input[i + 1..i + 1 + semi_rel];
                let decoded = match entity {
                    _ if entity.eq_ignore_ascii_case("amp") => Some("&".to_owned()),
                    _ if entity.eq_ignore_ascii_case("lt") => Some("<".to_owned()),
                    _ if entity.eq_ignore_ascii_case("gt") => Some(">".to_owned()),
                    _ if entity.eq_ignore_ascii_case("quot") => Some("\"".to_owned()),
                    _ if entity.eq_ignore_ascii_case("apos") => Some("'".to_owned()),
                    _ if entity.eq_ignore_ascii_case("nbsp") => Some(" ".to_owned()),
                    _ if entity.eq_ignore_ascii_case("ndash") => Some("-".to_owned()),
                    _ if entity.eq_ignore_ascii_case("mdash") => Some("—".to_owned()),
                    _ if entity.eq_ignore_ascii_case("hellip") => Some("…".to_owned()),
                    _ if entity.starts_with("#x") || entity.starts_with("#X") => {
                        u32::from_str_radix(&entity[2..], 16)
                            .ok()
                            .and_then(char::from_u32)
                            .filter(|c| !c.is_control() || c.is_whitespace())
                            .map(|c| c.to_string())
                    }
                    _ if entity.starts_with('#') => entity[1..]
                        .parse::<u32>()
                        .ok()
                        .and_then(char::from_u32)
                        .filter(|c| !c.is_control() || c.is_whitespace())
                        .map(|c| c.to_string()),
                    _ => None,
                };
                if let Some(replacement) = decoded {
                    out.push_str(&replacement);
                    i += 1 + semi_rel + 1;
                    continue;
                }
            }
        }
        let ch = input[i..].chars().next().unwrap_or('\0');
        out.push(ch);
        i += ch.len_utf8();
    }

    out
}

fn collapse_notification_whitespace(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut prev_space = true;
    for ch in input.chars() {
        if matches!(ch, '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{FEFF}') {
            continue;
        }
        if ch.is_control() && !ch.is_whitespace() {
            continue;
        }
        if ch.is_whitespace() || ch == '\u{00A0}' {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    if prev_space && out.ends_with(' ') {
        out.pop();
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IslandVisualState {
    Idle,
    Compact,
    Expanded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ListenerStatus {
    Starting,
    Listening,
    Paused,
    // Only produced by the WinRT listener; Linux D-Bus failures map to `Error`.
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    AccessDenied(String),
    #[allow(dead_code)] // Constructed by the Linux D-Bus listener and fallback targets.
    Error(String),
}

pub struct IslandState {
    pub active: Option<NotificationItem>,
    pub queue: VecDeque<NotificationItem>,
    pub visual: IslandVisualState,
    pub paused: bool,
    pub auto_hide: bool,
    pub hide_in_fullscreen: bool,
    pub fullscreen_active: bool,
    pub hovered: bool,
    idle_since: Instant,
    auto_hide_delay: Duration,
    collapse_deadline: Option<Instant>,
    collapse_timeout: Duration,
    pub listener_status: ListenerStatus,
}

impl IslandState {
    pub fn new(auto_hide: bool, collapse_timeout: Duration) -> Self {
        Self {
            active: None,
            queue: VecDeque::new(),
            visual: IslandVisualState::Idle,
            paused: false,
            auto_hide,
            hide_in_fullscreen: false,
            fullscreen_active: false,
            hovered: false,
            idle_since: Instant::now(),
            auto_hide_delay: Duration::ZERO,
            collapse_deadline: None,
            collapse_timeout,
            listener_status: ListenerStatus::Starting,
        }
    }

    pub fn push_notification(&mut self, mut item: NotificationItem, now: Instant) {
        if self.paused {
            return;
        }
        item.app_name = sanitize_notification_text(&item.app_name);
        if item.app_name.is_empty() {
            item.app_name = "System".to_owned();
        }
        let brave = is_brave_app(&item.app_name);
        item.title = clean_notification_field(&item.title, brave);
        if item.title.is_empty() {
            item.title = item.app_name.clone();
        }
        item.body = clean_notification_field(&item.body, brave);

        const MAX_BODY_BYTES: usize = 2000;
        if item.body.len() > MAX_BODY_BYTES {
            let mut cut = MAX_BODY_BYTES;
            while cut > 0 && !item.body.is_char_boundary(cut) {
                cut -= 1;
            }
            item.body.truncate(cut);
        }
        if let Some(active) = &mut self.active
            && active.id == item.id
        {
            *active = item;
            if !self.hovered {
                self.collapse_deadline = Some(now + self.collapse_timeout);
            }
            return;
        }
        if let Some(existing) = self.queue.iter_mut().find(|q| q.id == item.id) {
            *existing = item;
            return;
        }

        if self.active.is_none() {
            self.activate(item, now);
        } else {
            while self.queue.len() >= MAX_QUEUED_NOTIFICATIONS {
                self.queue.pop_front();
            }
            self.queue.push_back(item);
        }
    }

    pub fn dismiss_active(&mut self, now: Instant) -> Option<String> {
        let dismissed = self.active.take().map(|n| n.id);
        if let Some(next) = self.queue.pop_front() {
            self.activate(next, now);
        } else {
            self.visual = if self.hovered {
                IslandVisualState::Expanded
            } else {
                self.idle_since = now;
                IslandVisualState::Idle
            };
            self.collapse_deadline = None;
        }
        dismissed
    }

    pub fn remove_by_id(&mut self, id: &str, now: Instant) {
        if self.active.as_ref().is_some_and(|n| n.id == id) {
            self.dismiss_active(now);
            return;
        }
        self.queue.retain(|n| n.id != id);
    }

    pub fn set_hovered(&mut self, hovered: bool, now: Instant) -> bool {
        if self.hovered == hovered {
            return false;
        }
        self.hovered = hovered;
        if hovered {
            self.visual = IslandVisualState::Expanded;
            self.collapse_deadline = None;
        } else if self.active.is_some() {
            self.visual = IslandVisualState::Compact;
            self.collapse_deadline = Some(now + self.collapse_timeout);
        } else {
            self.visual = IslandVisualState::Idle;
            self.idle_since = now;
            self.collapse_deadline = None;
        }
        true
    }

    pub fn set_collapse_timeout(&mut self, timeout: Duration) {
        self.collapse_timeout = timeout;
    }

    pub fn set_auto_hide(&mut self, enabled: bool, now: Instant) {
        if self.auto_hide != enabled {
            self.auto_hide = enabled;
            self.idle_since = now;
        }
    }

    pub fn set_auto_hide_delay(&mut self, delay: Duration) {
        self.auto_hide_delay = delay;
    }

    pub fn set_hide_in_fullscreen(&mut self, enabled: bool) {
        self.hide_in_fullscreen = enabled;
    }

    pub fn set_fullscreen_active(&mut self, active: bool) -> bool {
        if self.fullscreen_active == active {
            return false;
        }
        self.fullscreen_active = active;
        self.idle_since = Instant::now();
        if active && self.hide_in_fullscreen && self.active.is_none() {
            self.hovered = false;
            self.visual = IslandVisualState::Idle;
        }
        true
    }

    pub fn auto_hide_deadline(&self) -> Option<Instant> {
        if self.auto_hide
            && self.active.is_none()
            && !self.hovered
            && !matches!(self.listener_status, ListenerStatus::AccessDenied(_))
        {
            Some(self.idle_since + self.auto_hide_delay)
        } else {
            None
        }
    }

    pub fn tick(&mut self, now: Instant) -> bool {
        if !self.hovered
            && let Some(deadline) = self.collapse_deadline
            && now >= deadline
        {
            self.dismiss_active(now);
            return true;
        }
        false
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
        if paused {
            self.active = None;
            self.queue.clear();
            self.visual = IslandVisualState::Idle;
            self.idle_since = Instant::now();
            self.collapse_deadline = None;
            if !matches!(
                self.listener_status,
                ListenerStatus::AccessDenied(_) | ListenerStatus::Error(_)
            ) {
                self.listener_status = ListenerStatus::Paused;
            }
        } else if matches!(self.listener_status, ListenerStatus::Paused) {
            self.listener_status = ListenerStatus::Listening;
        }
    }

    pub fn queued_count(&self) -> usize {
        self.queue.len()
    }

    pub fn should_be_visible(&self, now: Instant) -> bool {
        if self.hide_in_fullscreen && self.fullscreen_active && self.active.is_none() {
            return false;
        }
        if !self.auto_hide
            || self.active.is_some()
            || self.hovered
            || matches!(self.listener_status, ListenerStatus::AccessDenied(_))
        {
            return true;
        }
        now.saturating_duration_since(self.idle_since) < self.auto_hide_delay
    }

    pub fn next_deadline(&self) -> Option<Instant> {
        self.collapse_deadline
    }

    /// Fraction (`1.0` down to `0.0`) of the notification auto-dismiss countdown remaining.
    /// Returns `Some(1.0)` while hovered so the bar pauses cleanly, and `None` when idle.
    pub fn collapse_remaining_frac(&self, now: Instant) -> Option<f32> {
        self.active.as_ref()?;
        if self.hovered {
            return Some(1.0);
        }
        let deadline = self.collapse_deadline?;
        let total = self.collapse_timeout.as_secs_f32().max(0.05);
        let rem = deadline.saturating_duration_since(now).as_secs_f32();
        Some((rem / total).clamp(0.0, 1.0))
    }

    fn activate(&mut self, item: NotificationItem, now: Instant) {
        self.active = Some(item);
        if self.hovered {
            self.visual = IslandVisualState::Expanded;
            self.collapse_deadline = None;
        } else {
            self.visual = IslandVisualState::Compact;
            self.collapse_deadline = Some(now + self.collapse_timeout);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(id: &str, title: &str) -> NotificationItem {
        NotificationItem {
            id: id.to_owned(),
            app_name: "TestApp".to_owned(),
            title: title.to_owned(),
            body: format!("Body for {title}"),
            icon_rgba: None,
            timestamp: 1_700_000_000,
        }
    }

    #[test]
    fn push_transitions_idle_to_compact_and_queues_subsequent() {
        let t0 = Instant::now();
        let mut state = IslandState::new(false, Duration::from_secs(5));

        state.push_notification(sample("1", "First"), t0);
        assert_eq!(state.visual, IslandVisualState::Compact);
        assert_eq!(state.active.as_ref().map(|n| n.id.as_str()), Some("1"));
        assert_eq!(state.queued_count(), 0);

        state.push_notification(sample("2", "Second"), t0);
        assert_eq!(state.queued_count(), 1);

        // Dedup updates existing item instead of duplicating.
        state.push_notification(sample("2", "Second Updated"), t0);
        assert_eq!(state.queued_count(), 1);
        assert_eq!(state.queue[0].title, "Second Updated");
    }

    #[test]
    fn queue_is_bounded_for_unique_ids() {
        let t0 = Instant::now();
        let mut state = IslandState::new(false, Duration::from_secs(5));
        state.push_notification(sample("active", "Active"), t0);
        for i in 0..200 {
            state.push_notification(sample(&format!("q{i}"), "Queued"), t0);
        }
        assert_eq!(state.queued_count(), MAX_QUEUED_NOTIFICATIONS);
        assert_eq!(state.queue.front().map(|n| n.id.as_str()), Some("q168"));
        assert_eq!(state.queue.back().map(|n| n.id.as_str()), Some("q199"));
    }

    #[test]
    fn hover_expands_and_pauses_timeout_until_unhover() {
        let t0 = Instant::now();
        let mut state = IslandState::new(false, Duration::from_secs(5));
        state.push_notification(sample("1", "Ping"), t0);

        state.set_hovered(true, t0 + Duration::from_secs(2));
        assert_eq!(state.visual, IslandVisualState::Expanded);
        assert_eq!(state.next_deadline(), None);
        assert!(!state.tick(t0 + Duration::from_secs(10)));

        let unhover_at = t0 + Duration::from_secs(10);
        state.set_hovered(false, unhover_at);
        assert_eq!(state.visual, IslandVisualState::Compact);
        assert!(!state.tick(unhover_at + Duration::from_secs(4)));
        assert!(state.tick(unhover_at + Duration::from_secs(5)));
        assert_eq!(state.visual, IslandVisualState::Idle);
    }

    #[test]
    fn dismiss_advances_queue_and_returns_id() {
        let t0 = Instant::now();
        let mut state = IslandState::new(false, Duration::from_secs(5));
        state.push_notification(sample("1", "One"), t0);
        state.push_notification(sample("2", "Two"), t0);

        let dismissed = state.dismiss_active(t0 + Duration::from_secs(1));
        assert_eq!(dismissed.as_deref(), Some("1"));
        assert_eq!(state.active.as_ref().map(|n| n.id.as_str()), Some("2"));
        assert_eq!(state.visual, IslandVisualState::Compact);

        let dismissed_last = state.dismiss_active(t0 + Duration::from_secs(2));
        assert_eq!(dismissed_last.as_deref(), Some("2"));
        assert_eq!(state.active, None);
        assert_eq!(state.visual, IslandVisualState::Idle);
    }

    #[test]
    fn paused_ignores_incoming_and_auto_hide_visibility_works() {
        let t0 = Instant::now();
        let mut state = IslandState::new(true, Duration::from_secs(5));
        assert!(!state.should_be_visible(t0));

        state.push_notification(sample("1", "Visible"), t0);
        assert!(state.should_be_visible(t0));

        state.set_paused(true);
        assert_eq!(state.active, None);
        assert!(!state.should_be_visible(t0));

        state.push_notification(sample("2", "Ignored"), t0);
        assert_eq!(state.active, None);

        state.listener_status = ListenerStatus::AccessDenied("denied".to_owned());
        assert!(state.should_be_visible(t0));
    }

    #[test]
    fn auto_hide_waits_for_delay_and_hover_reveals_again() {
        let t0 = Instant::now();
        let mut state = IslandState::new(false, Duration::from_secs(5));
        state.set_auto_hide_delay(Duration::from_secs(4));
        state.set_auto_hide(true, t0);

        assert!(state.should_be_visible(t0 + Duration::from_secs(3)));
        assert!(!state.should_be_visible(t0 + Duration::from_secs(4)));

        let t_hover = t0 + Duration::from_secs(5);
        assert!(state.set_hovered(true, t_hover));
        assert!(state.should_be_visible(t_hover));

        let t_leave = t0 + Duration::from_secs(7);
        assert!(state.set_hovered(false, t_leave));
        assert!(state.should_be_visible(t_leave + Duration::from_secs(2)));
        assert!(!state.should_be_visible(t_leave + Duration::from_secs(4)));
    }

    #[test]
    fn fullscreen_hides_island_when_enabled_and_restores_on_exit() {
        let t0 = Instant::now();
        let mut state = IslandState::new(false, Duration::from_secs(5));
        state.set_hide_in_fullscreen(true);
        state.set_hovered(true, t0);
        assert!(state.should_be_visible(t0));

        assert!(state.set_fullscreen_active(true));
        assert!(!state.should_be_visible(t0));
        assert!(!state.hovered);

        // Notification arrives during fullscreen -> overlay pops up, then hides again after dismiss.
        state.push_notification(sample("1", "Ping"), t0);
        assert!(state.should_be_visible(t0));
        state.dismiss_active(t0 + Duration::from_secs(1));
        assert!(!state.should_be_visible(t0 + Duration::from_secs(1)));

        assert!(state.set_fullscreen_active(false));
        assert!(state.should_be_visible(t0 + Duration::from_secs(2)));

        state.set_hide_in_fullscreen(false);
        assert!(state.set_fullscreen_active(true));
        assert!(state.should_be_visible(t0 + Duration::from_secs(2)));
    }

    #[test]
    fn hover_in_idle_expands_and_unhover_collapses_to_idle() {
        let t0 = Instant::now();
        let mut state = IslandState::new(false, Duration::from_secs(5));
        assert_eq!(state.visual, IslandVisualState::Idle);

        assert!(state.set_hovered(true, t0));
        assert_eq!(state.visual, IslandVisualState::Expanded);
        assert_eq!(state.next_deadline(), None);

        assert!(state.set_hovered(false, t0 + Duration::from_secs(1)));
        assert_eq!(state.visual, IslandVisualState::Idle);
    }

    #[test]
    fn fullscreen_exit_resets_idle_since_for_auto_hide_delay() {
        let mut state = IslandState::new(true, Duration::from_secs(5));
        state.set_auto_hide_delay(Duration::from_secs(4));
        state.set_hide_in_fullscreen(true);

        assert!(state.set_fullscreen_active(true));
        assert!(!state.should_be_visible(Instant::now()));

        // When exiting fullscreen, idle_since resets so the notch stays visible for auto_hide_delay.
        assert!(state.set_fullscreen_active(false));
        assert!(state.should_be_visible(Instant::now()));
    }

    #[test]
    fn notification_queue_is_bounded_and_body_is_truncated() {
        let t0 = Instant::now();
        let mut state = IslandState::new(false, Duration::from_secs(5));
        let mut huge = sample("0", "Huge");
        huge.body = "a".repeat(3000);
        state.push_notification(huge, t0);
        assert_eq!(state.active.as_ref().unwrap().body.len(), 2000);

        for i in 1..=40 {
            state.push_notification(sample(&i.to_string(), "Burst"), t0);
        }
        assert_eq!(state.queued_count(), 32);
    }

    #[test]
    fn sanitize_strips_web_links_html_and_entities_while_preserving_plain_symbols() {
        assert_eq!(
            sanitize_notification_text(
                "<b>Budi</b><br/><a href=\"https://web.whatsapp.com\">Halo &amp; apa kabar?</a>"
            ),
            "Budi Halo & apa kabar?"
        );
        assert_eq!(
            sanitize_notification_text(
                "&lt;a href=&quot;https://web.whatsapp.com/&quot;&gt;Pesan baru&lt;/a&gt; &#x27;ok&#x27;&#32;&nbsp;!"
            ),
            "Pesan baru 'ok' !"
        );
        assert_eq!(
            sanitize_notification_text("Teks terpotong <a href=\"https://example.com"),
            "Teks terpotong"
        );
        assert_eq!(
            sanitize_notification_text("1 &lt; 2 &amp; I &lt;3 Rust <user@example.com>"),
            "1 < 2 & I <3 Rust <user@example.com>"
        );

        let t0 = Instant::now();
        let mut state = IslandState::new(false, Duration::from_secs(5));
        state.push_notification(
            NotificationItem {
                id: "wa-1".to_owned(),
                app_name: "<span>WhatsApp Web</span>".to_owned(),
                title: "<b>Grup</b> &amp; Teman".to_owned(),
                body:
                    "<a href=\"https://web.whatsapp.com\">https://web.whatsapp.com</a><br>Ayo rapat"
                        .to_owned(),
                icon_rgba: None,
                timestamp: 1,
            },
            t0,
        );
        let active = state.active.as_ref().expect("active notification");
        assert_eq!(active.app_name, "WhatsApp Web");
        assert_eq!(active.title, "Grup & Teman");
        assert_eq!(active.body, "https://web.whatsapp.com Ayo rapat");
    }

    #[test]
    fn brave_notifications_hide_web_origin_on_windows_and_linux() {
        let linux_wa = sanitized_notification_item(
            "br-1".to_owned(),
            "Brave",
            "Linux",
            "Budi (web.whatsapp.com)",
            "<a href=\"https://web.whatsapp.com\">web.whatsapp.com</a>\n\nHalo lagi di jalan",
            None,
            10,
        );
        assert_eq!(linux_wa.title, "Budi");
        assert_eq!(linux_wa.body, "Halo lagi di jalan");

        let br_inline = sanitized_notification_item(
            "br-2".to_owned(),
            "Brave Browser",
            "Windows",
            "Live Stream — youtube.com",
            "via chat.openai.com\nSelesai v0.2.0 jam 10.30",
            None,
            11,
        );
        assert_eq!(br_inline.title, "Live Stream");
        assert_eq!(br_inline.body, "Selesai v0.2.0 jam 10.30");
    }

    #[test]
    fn collapse_remaining_frac_shrinks_and_pauses_on_hover() {
        let t0 = Instant::now();
        let mut state = IslandState::new(false, Duration::from_secs(5));
        assert_eq!(state.collapse_remaining_frac(t0), None);
        state.push_notification(sample("c-1", "Countdown"), t0);
        let half = state
            .collapse_remaining_frac(t0 + Duration::from_millis(2500))
            .expect("active bar frac");
        assert!((half - 0.5).abs() < 0.05);
        state.set_hovered(true, t0 + Duration::from_millis(2500));
        assert_eq!(
            state.collapse_remaining_frac(t0 + Duration::from_secs(4)),
            Some(1.0)
        );
    }
}
