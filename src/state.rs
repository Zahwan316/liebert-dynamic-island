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
}
