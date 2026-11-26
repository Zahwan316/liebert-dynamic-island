use std::io::Write;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::calendar::{CalendarCommand, CalendarEvent};
use crate::config::Config;
use crate::lyrics::{LyricLine, LyricsData, LyricsRequest, PlainLyricLine};
use crate::platform::{AlbumArt, MediaCommand, MediaTrack, RepeatMode};
use crate::state::{ListenerStatus, NotificationItem};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpcAlbumArt {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
    pub dominant_rgb: Option<[u8; 3]>,
}

impl From<&AlbumArt> for IpcAlbumArt {
    fn from(art: &AlbumArt) -> Self {
        Self {
            width: art.width,
            height: art.height,
            rgba: art.rgba.clone(),
            dominant_rgb: art.dominant_rgb,
        }
    }
}

impl From<IpcAlbumArt> for AlbumArt {
    fn from(art: IpcAlbumArt) -> Self {
        Self {
            width: art.width,
            height: art.height,
            rgba: art.rgba,
            dominant_rgb: art.dominant_rgb,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpcMediaTrack {
    pub title: String,
    pub artist: String,
    pub is_playing: bool,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub shuffle: bool,
    pub repeat: RepeatMode,
    pub album_art: Option<IpcAlbumArt>,
    pub elapsed_ms: u64,
}

impl From<&MediaTrack> for IpcMediaTrack {
    fn from(track: &MediaTrack) -> Self {
        Self {
            title: track.title.clone(),
            artist: track.artist.clone(),
            is_playing: track.is_playing,
            position_ms: track.position_ms,
            duration_ms: track.duration_ms,
            shuffle: track.shuffle,
            repeat: track.repeat,
            album_art: track.album_art.as_deref().map(IpcAlbumArt::from),
            elapsed_ms: track.updated_at.elapsed().as_millis().min(u64::MAX as u128) as u64,
        }
    }
}

impl From<IpcMediaTrack> for MediaTrack {
    fn from(track: IpcMediaTrack) -> Self {
        let now = Instant::now();
        let updated_at = now
            .checked_sub(Duration::from_millis(track.elapsed_ms))
            .unwrap_or(now);
        Self {
            title: track.title,
            artist: track.artist,
            is_playing: track.is_playing,
            position_ms: track.position_ms,
            duration_ms: track.duration_ms,
            shuffle: track.shuffle,
            repeat: track.repeat,
            album_art: track.album_art.map(|a| Arc::new(AlbumArt::from(a))),
            updated_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IpcLyricsData {
    Loading,
    Synced(Vec<LyricLine>),
    Plain(Vec<PlainLyricLine>),
    NotFound,
}

impl From<&LyricsData> for IpcLyricsData {
    fn from(data: &LyricsData) -> Self {
        match data {
            LyricsData::Loading => Self::Loading,
            LyricsData::Synced(lines) => Self::Synced(lines.to_vec()),
            LyricsData::Plain(lines) => Self::Plain(lines.to_vec()),
            LyricsData::NotFound => Self::NotFound,
        }
    }
}

impl From<IpcLyricsData> for LyricsData {
    fn from(data: IpcLyricsData) -> Self {
        match data {
            IpcLyricsData::Loading => Self::Loading,
            IpcLyricsData::Synced(lines) => Self::Synced(Arc::from(lines)),
            IpcLyricsData::Plain(lines) => Self::Plain(Arc::from(lines)),
            IpcLyricsData::NotFound => Self::NotFound,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DaemonSnapshot {
    pub config: Config,
    pub paused: bool,
    pub listener_status: ListenerStatus,
    pub active_notification: Option<NotificationItem>,
    pub media: Option<IpcMediaTrack>,
    pub lyrics: IpcLyricsData,
    pub lyrics_key: Option<(String, String)>,
    pub calendar_events: Vec<CalendarEvent>,
    pub calendar_status: String,
    pub fullscreen_exclusive: bool,
    pub fullscreen_windowed: bool,
    pub open_settings: bool,
    pub test_notification: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DaemonToUi {
    Snapshot(Box<DaemonSnapshot>),
    Notification(NotificationItem),
    Removed(String),
    Status(ListenerStatus),
    Media(Option<IpcMediaTrack>),
    Fullscreen(bool, bool),
    LyricsLoaded {
        artist: String,
        title: String,
        lyrics: IpcLyricsData,
    },
    CalendarUpdated(Result<Vec<CalendarEvent>, String>),
    ConfigUpdated(Config),
    SetPaused(bool),
    OpenSettings,
    TestNotification,
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UiToDaemon {
    MediaCommand(MediaCommand),
    CalendarCommand(CalendarCommand),
    LyricsRequest(LyricsRequest),
    ConfigSaved(Config),
    DismissNotification,
    TogglePause,
    TestNotification,
    UiClosing,
    ExitDaemon,
}

pub fn write_msg<T: Serialize>(writer: &mut impl Write, msg: &T) -> std::io::Result<()> {
    let encoded = serde_json::to_string(msg).map_err(std::io::Error::other)?;
    writer.write_all(encoded.as_bytes())?;
    writer.write_all(b"\n")?;
    writer.flush()
}

pub fn read_msg<T: DeserializeOwned>(line: &str) -> serde_json::Result<T> {
    serde_json::from_str(line.trim_end_matches(['\r', '\n']))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_and_commands_round_trip() {
        let snap = DaemonSnapshot {
            config: Config::default(),
            paused: false,
            listener_status: ListenerStatus::Listening,
            active_notification: Some(NotificationItem {
                id: "n-1".to_owned(),
                app_name: "Test".to_owned(),
                title: "Hello".to_owned(),
                body: "World".to_owned(),
                icon_rgba: None,
                timestamp: 42,
            }),
            media: None,
            lyrics: IpcLyricsData::NotFound,
            lyrics_key: None,
            calendar_events: Vec::new(),
            calendar_status: "Idle".to_owned(),
            fullscreen_exclusive: false,
            fullscreen_windowed: false,
            open_settings: true,
            test_notification: false,
        };
        let msg = DaemonToUi::Snapshot(Box::new(snap));
        let mut buf = Vec::new();
        write_msg(&mut buf, &msg).expect("serialize snapshot");
        let line = String::from_utf8(buf).expect("utf8");
        let decoded: DaemonToUi = read_msg(&line).expect("deserialize snapshot");
        assert_eq!(decoded, msg);

        let cmd = UiToDaemon::MediaCommand(MediaCommand::TogglePlayPause);
        let mut cmd_buf = Vec::new();
        write_msg(&mut cmd_buf, &cmd).expect("serialize cmd");
        let cmd_line = String::from_utf8(cmd_buf).expect("utf8");
        let decoded_cmd: UiToDaemon = read_msg(&cmd_line).expect("deserialize cmd");
        assert_eq!(decoded_cmd, cmd);
    }
}
