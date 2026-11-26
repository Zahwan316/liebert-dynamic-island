use std::collections::HashSet;
use std::io::{BufRead, BufReader};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, ExitCode};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use crate::calendar::{
    self, CalendarCommand, CalendarEvent, format_countdown, format_local_time, upcoming_events,
};
use crate::config::{Config, KaraokeLyricMode};
use crate::ipc::{
    DaemonSnapshot, DaemonToUi, IpcLyricsData, IpcMediaTrack, UiToDaemon, read_msg, write_msg,
};
use crate::lyrics::{self, LyricsData, LyricsRequest};
use crate::platform::{self, MediaTrack, PlatformEvent};
use crate::pomodoro::{PomodoroPhase, PomodoroTimer};
use crate::state::{IslandState, NotificationItem};
use crate::tray::{TrayAction, TrayController};

const DAEMON_TICK: Duration = Duration::from_millis(50);
const UI_SPAWN_COOLDOWN: Duration = Duration::from_millis(600);

enum AcceptMsg {
    Connected(TcpStream),
}

struct DaemonState {
    config: Config,
    state: IslandState,
    tray: Option<TrayController>,
    media: Option<MediaTrack>,
    lyrics: LyricsData,
    lyrics_key: Option<(String, String)>,
    lyrics_had_duration: bool,
    calendar_events: Vec<CalendarEvent>,
    calendar_status: String,
    alerted_events: HashSet<(String, i64)>,
    pomodoro: PomodoroTimer,
    fullscreen_exclusive: bool,
    fullscreen_windowed: bool,
    test_notif_seq: u32,
    demo_pending: bool,
    pending_open_settings: bool,
    pending_test_notif: bool,
    ui_child: Option<Child>,
    ui_conn: Option<TcpStream>,
    last_ui_spawn: Option<Instant>,
}

impl DaemonState {
    fn new(config: Config, tray: Option<TrayController>, demo_mode: bool) -> Self {
        let timeout = Duration::from_secs(config.collapse_timeout_secs.max(1));
        let mut state = IslandState::new(config.auto_hide_when_idle, timeout);
        state.set_auto_hide_delay(Duration::from_secs(config.auto_hide_delay_secs));
        state.set_hide_in_fullscreen(config.hide_in_fullscreen);
        let pomodoro = PomodoroTimer::new(
            config.pomodoro_focus_mins,
            config.pomodoro_short_break_mins,
            config.pomodoro_long_break_mins,
        );
        Self {
            config,
            state,
            tray,
            media: None,
            lyrics: LyricsData::NotFound,
            lyrics_key: None,
            lyrics_had_duration: false,
            calendar_events: Vec::new(),
            calendar_status: "Idle".to_owned(),
            alerted_events: HashSet::new(),
            pomodoro,
            fullscreen_exclusive: false,
            fullscreen_windowed: false,
            test_notif_seq: 0,
            demo_pending: demo_mode,
            pending_open_settings: false,
            pending_test_notif: false,
            ui_child: None,
            ui_conn: None,
            last_ui_spawn: None,
        }
    }

    fn sync_tray(&self) {
        if let Some(tray) = &self.tray {
            tray.update_status(&self.state.listener_status, self.state.paused);
            tray.set_auto_hide_checked(self.config.auto_hide_when_idle);
            tray.update_dock_mode(self.config.notch_dock_mode);
        }
    }

    fn send_to_ui(&mut self, msg: &DaemonToUi) {
        let Some(conn) = &mut self.ui_conn else {
            return;
        };
        if write_msg(conn, msg).is_err() {
            self.ui_conn = None;
        }
    }

    fn build_snapshot(&mut self) -> DaemonSnapshot {
        let open_settings = std::mem::take(&mut self.pending_open_settings);
        let test_notification = std::mem::take(&mut self.pending_test_notif);
        DaemonSnapshot {
            config: self.config.clone(),
            paused: self.state.paused,
            listener_status: self.state.listener_status.clone(),
            active_notification: self.state.active.clone(),
            media: self.media.as_ref().map(IpcMediaTrack::from),
            lyrics: IpcLyricsData::from(&self.lyrics),
            lyrics_key: self.lyrics_key.clone(),
            calendar_events: self.calendar_events.clone(),
            calendar_status: self.calendar_status.clone(),
            fullscreen_exclusive: self.fullscreen_exclusive,
            fullscreen_windowed: self.fullscreen_windowed,
            open_settings,
            test_notification,
        }
    }

    fn push_test_notification(&mut self, now: Instant) {
        self.test_notif_seq = self.test_notif_seq.wrapping_add(1);
        let item = NotificationItem {
            id: format!("test-{}", self.test_notif_seq),
            app_name: "Liebert Test".to_owned(),
            title: format!("Test Notification #{}", self.test_notif_seq),
            body: "Hover to expand, click Dismiss, or open Settings from tray.".to_owned(),
            icon_rgba: None,
            timestamp: chrono::Utc::now().timestamp().max(0) as u64,
        };
        self.state.push_notification(item.clone(), now);
        self.send_to_ui(&DaemonToUi::Notification(item));
    }

    fn is_hidden_by_fullscreen(&self) -> bool {
        self.state.hide_in_fullscreen
            && self.state.fullscreen_active
            && self.state.active.is_none()
    }

    fn should_spawn_ui(&self, now: Instant) -> bool {
        if self.ui_child.is_some() || self.ui_conn.is_some() {
            return false;
        }
        if let Some(last) = self.last_ui_spawn
            && now.duration_since(last) < UI_SPAWN_COOLDOWN
        {
            return false;
        }
        if self.pending_open_settings || self.pending_test_notif || self.demo_pending {
            return true;
        }
        if self.state.active.is_some() && self.config.show_notifications {
            return true;
        }
        if self.is_hidden_by_fullscreen() {
            return false;
        }
        if !self.config.auto_hide_when_idle {
            return true;
        }
        let top_y = if self.config.notch_dock_mode {
            0.0
        } else {
            self.config.top_offset_px
        };
        let trigger_w = self.config.idle_width.max(180.0) * self.config.scale;
        platform::is_cursor_in_top_trigger(top_y, trigger_w, 1.0).unwrap_or(false)
    }

    fn spawn_ui(&mut self, ipc_addr: &str, now: Instant) {
        let Ok(exe) = std::env::current_exe() else {
            eprintln!("Failed to resolve current executable for UI spawn");
            return;
        };
        let mut cmd = Command::new(exe);
        cmd.arg("--ui").arg("--ipc-addr").arg(ipc_addr);
        if self.demo_pending {
            cmd.arg("--demo");
            self.demo_pending = false;
        }
        if self.pending_open_settings {
            cmd.arg("--settings");
        }
        match cmd.spawn() {
            Ok(child) => {
                self.ui_child = Some(child);
                self.last_ui_spawn = Some(now);
            }
            Err(err) => {
                eprintln!("Failed to spawn UI process: {err}");
            }
        }
    }

    fn reap_ui_child(&mut self) {
        if let Some(child) = &mut self.ui_child
            && let Ok(Some(_)) = child.try_wait()
        {
            self.ui_child = None;
            self.ui_conn = None;
        }
    }

    fn request_lyrics_if_needed(&mut self, lyrics_req_tx: &mpsc::Sender<LyricsRequest>) {
        if !self.config.show_lyrics {
            return;
        }
        let Some(track) = &self.media else {
            return;
        };
        let translate_lang = if self.config.translate_lyrics
            && (self.config.show_pill_translation
                || (self.config.karaoke_lyrics
                    && self.config.karaoke_lyric_mode == KaraokeLyricMode::Translate))
            && !self.config.lyric_translate_lang.trim().is_empty()
        {
            Some(self.config.lyric_translate_lang.clone())
        } else {
            None
        };
        let need_romaji = self.config.romaji_lyrics
            || (self.config.karaoke_lyrics
                && self.config.karaoke_lyric_mode == KaraokeLyricMode::Romaji);
        let _ = lyrics_req_tx.send(LyricsRequest {
            artist: track.artist.clone(),
            title: track.title.clone(),
            duration_secs: (track.duration_ms > 0).then_some(track.duration_ms / 1000),
            translate_lang,
            need_romaji,
        });
    }
}

pub fn run_daemon(demo_mode: bool) -> ExitCode {
    let config = Config::load();
    let tray = match TrayController::new(config.auto_hide_when_idle, config.notch_dock_mode) {
        Ok(t) => Some(t),
        Err(err) => {
            eprintln!("Warning: tray unavailable ({err})");
            None
        }
    };

    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(l) => l,
        Err(err) => {
            eprintln!("Fatal IPC bind error: {err}");
            return ExitCode::FAILURE;
        }
    };
    let Ok(local_addr) = listener.local_addr() else {
        eprintln!("Fatal IPC address error");
        return ExitCode::FAILURE;
    };
    let ipc_addr = local_addr.to_string();

    let (accept_tx, accept_rx) = mpsc::channel::<AcceptMsg>();
    let (ui_msg_tx, ui_msg_rx) = mpsc::channel::<UiToDaemon>();

    let _ = std::thread::Builder::new()
        .name("ipc-accept".to_owned())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                let _ = stream.set_nodelay(true);
                if let Ok(read_stream) = stream.try_clone() {
                    let tx = ui_msg_tx.clone();
                    let _ = std::thread::Builder::new()
                        .name("ipc-reader".to_owned())
                        .spawn(move || {
                            let reader = BufReader::new(read_stream);
                            for line in reader.lines().map_while(Result::ok) {
                                if line.trim().is_empty() {
                                    continue;
                                }
                                if let Ok(msg) = read_msg::<UiToDaemon>(&line)
                                    && tx.send(msg).is_err()
                                {
                                    break;
                                }
                            }
                        });
                }
                if accept_tx.send(AcceptMsg::Connected(stream)).is_err() {
                    break;
                }
            }
        });

    let (event_tx, event_rx) = mpsc::channel();
    let (lyrics_req_tx, lyrics_req_rx) = mpsc::channel();
    let (media_cmd_tx, media_cmd_rx) = mpsc::channel();
    let (calendar_cmd_tx, calendar_cmd_rx) = mpsc::channel();

    let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(|| {});
    platform::spawn_listener(event_tx.clone(), wake.clone());
    platform::spawn_media_listener(event_tx.clone(), media_cmd_rx, wake.clone());
    platform::spawn_fullscreen_watcher(event_tx.clone(), wake.clone());
    lyrics::spawn_lyrics_worker(lyrics_req_rx, event_tx.clone(), wake.clone());
    calendar::spawn_calendar_worker(
        config.google_calendar_ics_url.clone(),
        calendar_cmd_rx,
        event_tx,
        wake,
    );

    let mut daemon = DaemonState::new(config, tray, demo_mode);
    daemon.sync_tray();

    let mut should_exit = false;
    while !should_exit {
        let now = Instant::now();
        platform::pump_os_events();
        daemon.reap_ui_child();

        while let Ok(AcceptMsg::Connected(mut stream)) = accept_rx.try_recv() {
            let snapshot = daemon.build_snapshot();
            if write_msg(&mut stream, &DaemonToUi::Snapshot(Box::new(snapshot))).is_ok() {
                daemon.ui_conn = Some(stream);
            }
        }

        while let Ok(ev) = event_rx.try_recv() {
            match ev {
                PlatformEvent::Notification(item) => {
                    if daemon.config.show_notifications {
                        daemon.state.push_notification(item.clone(), now);
                        daemon.send_to_ui(&DaemonToUi::Notification(item));
                    }
                }
                PlatformEvent::Removed(id) => {
                    daemon.state.remove_by_id(&id, now);
                    daemon.send_to_ui(&DaemonToUi::Removed(id));
                }
                PlatformEvent::Status(status) => {
                    daemon.state.listener_status = status.clone();
                    daemon.sync_tray();
                    daemon.send_to_ui(&DaemonToUi::Status(status));
                }
                PlatformEvent::Fullscreen(exclusive, windowed) => {
                    daemon.fullscreen_exclusive = exclusive;
                    daemon.fullscreen_windowed = windowed;
                    let fs = daemon.config.hide_in_fullscreen
                        && (exclusive
                            || (daemon.config.hide_in_windowed_fullscreen && windowed));
                    daemon.state.set_fullscreen_active(fs);
                    daemon.send_to_ui(&DaemonToUi::Fullscreen(exclusive, windowed));
                }
                PlatformEvent::Media(track) => {
                    let ipc_track = track.as_ref().map(IpcMediaTrack::from);
                    daemon.media = track;
                    if let Some((artist, title, has_dur)) = daemon
                        .media
                        .as_ref()
                        .map(|t| (t.artist.clone(), t.title.clone(), t.duration_ms > 0))
                    {
                        let key = (artist, title);
                        let key_changed = daemon.lyrics_key.as_ref() != Some(&key);
                        let duration_arrived = !key_changed
                            && has_dur
                            && !daemon.lyrics_had_duration
                            && matches!(daemon.lyrics, LyricsData::NotFound | LyricsData::Plain(_));
                        if key_changed || duration_arrived {
                            daemon.lyrics_key = Some(key);
                            daemon.lyrics_had_duration = has_dur;
                            daemon.lyrics = LyricsData::Loading;
                            daemon.request_lyrics_if_needed(&lyrics_req_tx);
                        }
                    } else {
                        daemon.lyrics_key = None;
                        daemon.lyrics_had_duration = false;
                        daemon.lyrics = LyricsData::NotFound;
                    }
                    daemon.send_to_ui(&DaemonToUi::Media(ipc_track));
                }
                PlatformEvent::LyricsLoaded {
                    artist,
                    title,
                    lyrics,
                } => {
                    if daemon.lyrics_key.as_ref() == Some(&(artist.clone(), title.clone())) {
                        daemon.lyrics = lyrics.clone();
                        daemon.send_to_ui(&DaemonToUi::LyricsLoaded {
                            artist,
                            title,
                            lyrics: IpcLyricsData::from(&lyrics),
                        });
                    }
                }
                PlatformEvent::CalendarUpdated(res) => {
                    match &res {
                        Ok(events) => {
                            daemon.calendar_status = format!("Synced ({} events)", events.len());
                            daemon.calendar_events = events.clone();
                        }
                        Err(err) => {
                            daemon.calendar_status = err.clone();
                        }
                    }
                    daemon.send_to_ui(&DaemonToUi::CalendarUpdated(res));
                }
            }
        }

        while let Ok(msg) = ui_msg_rx.try_recv() {
            match msg {
                UiToDaemon::MediaCommand(cmd) => {
                    let _ = media_cmd_tx.send(cmd);
                }
                UiToDaemon::CalendarCommand(cmd) => {
                    let _ = calendar_cmd_tx.send(cmd);
                }
                UiToDaemon::LyricsRequest(req) => {
                    let _ = lyrics_req_tx.send(req);
                }
                UiToDaemon::ConfigSaved(cfg) => {
                    let prev_url = daemon.config.google_calendar_ics_url.clone();
                    daemon.config = cfg;
                    daemon
                        .state
                        .set_auto_hide(daemon.config.auto_hide_when_idle, now);
                    daemon
                        .state
                        .set_auto_hide_delay(Duration::from_secs(daemon.config.auto_hide_delay_secs));
                    daemon
                        .state
                        .set_hide_in_fullscreen(daemon.config.hide_in_fullscreen);
                    daemon
                        .state
                        .set_collapse_timeout(Duration::from_secs(daemon.config.collapse_timeout_secs));
                    daemon.sync_tray();
                    if daemon.config.google_calendar_ics_url != prev_url {
                        let _ = calendar_cmd_tx.send(CalendarCommand::SetUrl(
                            daemon.config.google_calendar_ics_url.clone(),
                        ));
                    }
                    if let Err(err) = daemon.config.save() {
                        eprintln!("Failed to save config from UI: {err}");
                    }
                }
                UiToDaemon::DismissNotification => {
                    daemon.state.dismiss_active(now);
                }
                UiToDaemon::TogglePause => {
                    daemon.state.set_paused(!daemon.state.paused);
                    daemon.sync_tray();
                    let paused = daemon.state.paused;
                    daemon.send_to_ui(&DaemonToUi::SetPaused(paused));
                }
                UiToDaemon::TestNotification => {
                    daemon.push_test_notification(now);
                }
                UiToDaemon::UiClosing => {
                    daemon.ui_conn = None;
                }
                UiToDaemon::ExitDaemon => {
                    should_exit = true;
                }
            }
        }

        let tray_events = daemon
            .tray
            .as_ref()
            .map(|tray| (tray.poll_actions(), tray.is_auto_hide_checked()));
        if let Some((actions, auto_hide_checked)) = tray_events {
            for action in actions {
                match action {
                    TrayAction::OpenSettings => {
                        if daemon.ui_conn.is_some() {
                            daemon.send_to_ui(&DaemonToUi::OpenSettings);
                        } else {
                            daemon.pending_open_settings = true;
                        }
                    }
                    TrayAction::ToggleDockMode => {
                        daemon.config.notch_dock_mode = !daemon.config.notch_dock_mode;
                        daemon.sync_tray();
                        if let Err(err) = daemon.config.save() {
                            eprintln!("Failed to save config: {err}");
                        }
                        let cfg = daemon.config.clone();
                        daemon.send_to_ui(&DaemonToUi::ConfigUpdated(cfg));
                    }
                    TrayAction::TestNotification => {
                        if daemon.ui_conn.is_some() {
                            daemon.push_test_notification(now);
                        } else {
                            daemon.pending_test_notif = true;
                        }
                    }
                    TrayAction::TogglePause => {
                        daemon.state.set_paused(!daemon.state.paused);
                        daemon.sync_tray();
                        let paused = daemon.state.paused;
                        daemon.send_to_ui(&DaemonToUi::SetPaused(paused));
                    }
                    TrayAction::ToggleAutoHide => {
                        daemon.config.auto_hide_when_idle = auto_hide_checked;
                        daemon.state.set_auto_hide(auto_hide_checked, now);
                        if let Err(err) = daemon.config.save() {
                            eprintln!("Failed to save config: {err}");
                        }
                        let cfg = daemon.config.clone();
                        daemon.send_to_ui(&DaemonToUi::ConfigUpdated(cfg));
                    }
                    TrayAction::Exit => {
                        daemon.send_to_ui(&DaemonToUi::Shutdown);
                        should_exit = true;
                    }
                }
            }
        }

        daemon.state.tick(now);

        if daemon.config.show_calendar && !daemon.calendar_events.is_empty() {
            let now_utc = chrono::Utc::now().timestamp();
            let mut due_alerts = Vec::new();
            for ev in upcoming_events(&daemon.calendar_events, now_utc, 3) {
                let secs_until = ev.start_utc - now_utc;
                if (0..=300).contains(&secs_until) {
                    let key = (ev.summary.clone(), ev.start_utc);
                    if daemon.alerted_events.insert(key) {
                        due_alerts.push(NotificationItem {
                            id: format!("cal-{}-{}", ev.start_utc, ev.summary),
                            app_name: "Google Calendar".to_owned(),
                            title: ev.summary.clone(),
                            body: format!(
                                "{} • {}",
                                format_local_time(ev.start_utc),
                                format_countdown(ev.start_utc, ev.end_utc, now_utc)
                            ),
                            icon_rgba: None,
                            timestamp: now_utc.max(0) as u64,
                        });
                    }
                }
            }
            for item in due_alerts {
                daemon.state.push_notification(item.clone(), now);
                daemon.send_to_ui(&DaemonToUi::Notification(item));
            }
        }

        if let Some(finished_phase) = daemon.pomodoro.tick(now) {
            let (title, body) = match finished_phase {
                PomodoroPhase::Focus => (
                    "Focus Session Complete",
                    format!(
                        "Completed {} focus session(s). Time for a {}!",
                        daemon.pomodoro.completed_focus,
                        daemon.pomodoro.phase.label()
                    ),
                ),
                PomodoroPhase::ShortBreak | PomodoroPhase::LongBreak => (
                    "Break Finished",
                    "Ready to start the next Focus session.".to_owned(),
                ),
            };
            let item = NotificationItem {
                id: format!("pomodoro-{}", daemon.pomodoro.completed_focus),
                app_name: "Pomodoro".to_owned(),
                title: title.to_owned(),
                body,
                icon_rgba: None,
                timestamp: chrono::Utc::now().timestamp().max(0) as u64,
            };
            daemon.state.push_notification(item.clone(), now);
            daemon.send_to_ui(&DaemonToUi::Notification(item));
        }

        if !should_exit && daemon.should_spawn_ui(now) {
            daemon.spawn_ui(&ipc_addr, now);
        }

        std::thread::sleep(DAEMON_TICK);
    }

    if let Some(mut child) = daemon.ui_child.take() {
        let deadline = Instant::now() + Duration::from_millis(500);
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(25));
                }
                _ => {
                    let _ = child.kill();
                    break;
                }
            }
        }
    }

    ExitCode::SUCCESS
}
