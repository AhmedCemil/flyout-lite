use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use windows::{
    core::*,
    Foundation::{TimeSpan, TypedEventHandler},
    Media::{
        Control::{
            CurrentSessionChangedEventArgs,
            GlobalSystemMediaTransportControlsSession,
            GlobalSystemMediaTransportControlsSessionManager,
            GlobalSystemMediaTransportControlsSessionPlaybackStatus,
            MediaPropertiesChangedEventArgs,
            PlaybackInfoChangedEventArgs,
            TimelinePropertiesChangedEventArgs,
        },
        MediaPlaybackAutoRepeatMode,
    },
    Storage::Streams::{Buffer, DataReader, IRandomAccessStreamWithContentType, InputStreamOptions},
    Win32::{
        Foundation::{HWND, LPARAM, WPARAM},
        UI::WindowsAndMessaging::PostMessageW,
    },
};

use crate::app;
use crate::config;
use crate::hotkey::WM_APP_HOTKEY;

static NOTIFY_HWND: AtomicIsize = AtomicIsize::new(0);
static LAST_TITLE: Mutex<String> = Mutex::new(String::new());
static LAST_ARTIST: Mutex<String> = Mutex::new(String::new());
static LAST_PLAYING: std::sync::atomic::AtomicI8 = std::sync::atomic::AtomicI8::new(-1);

pub fn set_notify_target(hwnd: HWND) {
    NOTIFY_HWND.store(hwnd.0 as isize, Ordering::Release);
}

fn post_show_if_enabled() {
    if !config::get().show_on_session_change {
        return;
    }
    let target = NOTIFY_HWND.load(Ordering::Acquire);
    if target == 0 {
        return;
    }
    unsafe {
        let _ = PostMessageW(
            Some(HWND(target as *mut _)),
            WM_APP_HOTKEY,
            WPARAM(0),
            LPARAM(0),
        );
    }
}

static MANAGER: OnceLock<Mutex<Option<GlobalSystemMediaTransportControlsSessionManager>>> =
    OnceLock::new();

fn manager_slot() -> &'static Mutex<Option<GlobalSystemMediaTransportControlsSessionManager>> {
    MANAGER.get_or_init(|| Mutex::new(None))
}

pub fn current_session() -> Option<GlobalSystemMediaTransportControlsSession> {
    let guard = manager_slot().lock().ok()?;
    guard.as_ref()?.GetCurrentSession().ok()
}

pub fn subscribe() -> Result<()> {
    let op = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()?;
    let manager = op.get()?;

    if let Ok(session) = manager.GetCurrentSession() {
        refresh_all(&session);
        attach_session_handlers(&session);
    }

    let session_changed = TypedEventHandler::<
        GlobalSystemMediaTransportControlsSessionManager,
        CurrentSessionChangedEventArgs,
    >::new(move |mgr, _| {
        let m = mgr.ok()?;
        if let Ok(s) = m.GetCurrentSession() {
            refresh_all(&s);
            attach_session_handlers(&s);
        } else {
            app::clear_thumbnail();
            app::update_track(String::new(), String::new());
            app::update_playing(false);
            app::update_timeline(0.0, 0.0, false);
        }
        Ok(())
    });
    manager.CurrentSessionChanged(&session_changed)?;

    if let Ok(mut slot) = manager_slot().lock() {
        *slot = Some(manager);
    }

    Ok(())
}

fn attach_session_handlers(session: &GlobalSystemMediaTransportControlsSession) {
    let props_handler = TypedEventHandler::<
        GlobalSystemMediaTransportControlsSession,
        MediaPropertiesChangedEventArgs,
    >::new(|session, _| {
        let s = session.ok()?;
        refresh_properties(s);
        Ok(())
    });

    let playback_handler = TypedEventHandler::<
        GlobalSystemMediaTransportControlsSession,
        PlaybackInfoChangedEventArgs,
    >::new(|session, _| {
        let s = session.ok()?;
        refresh_playback(s);
        Ok(())
    });

    let timeline_handler = TypedEventHandler::<
        GlobalSystemMediaTransportControlsSession,
        TimelinePropertiesChangedEventArgs,
    >::new(|session, _| {
        let s = session.ok()?;
        refresh_timeline(s);
        Ok(())
    });

    let _ = session.MediaPropertiesChanged(&props_handler);
    let _ = session.PlaybackInfoChanged(&playback_handler);
    let _ = session.TimelinePropertiesChanged(&timeline_handler);
}

fn refresh_all(session: &GlobalSystemMediaTransportControlsSession) {
    refresh_properties(session);
    refresh_playback(session);
    refresh_timeline(session);
}

fn refresh_properties(session: &GlobalSystemMediaTransportControlsSession) {
    let Ok(op) = session.TryGetMediaPropertiesAsync() else { return };
    let Ok(props) = op.get() else { return };

    let title = props.Title().unwrap_or_default().to_string_lossy();
    let artist = props.Artist().unwrap_or_default().to_string_lossy();
    let key = format!("{title}|{artist}");

    let track_changed = {
        let mut lt = LAST_TITLE.lock().unwrap();
        let mut la = LAST_ARTIST.lock().unwrap();
        let changed = (*lt != title || *la != artist) && !(title.is_empty() && artist.is_empty());
        let first_seen = lt.is_empty() && la.is_empty();
        *lt = title.clone();
        *la = artist.clone();
        changed && !first_seen
    };

    app::update_track(title, artist);
    if track_changed {
        post_show_if_enabled();
    }

    if let Ok(thumb_ref) = props.Thumbnail() {
        if let Ok(stream_op) = thumb_ref.OpenReadAsync() {
            if let Ok(stream) = stream_op.get() {
                if let Some(bytes) = read_stream_bytes(&stream) {
                    app::update_thumbnail(key, bytes);
                    return;
                }
            }
        }
    }
    app::clear_thumbnail();
}

fn read_stream_bytes(stream: &IRandomAccessStreamWithContentType) -> Option<Vec<u8>> {
    let size = stream.Size().ok()? as u32;
    if size == 0 {
        return None;
    }
    let input = stream.GetInputStreamAt(0).ok()?;
    let buffer = Buffer::Create(size).ok()?;
    let op = input
        .ReadAsync(&buffer, size, InputStreamOptions::None)
        .ok()?;
    let filled = op.get().ok()?;
    let reader = DataReader::FromBuffer(&filled).ok()?;
    let len = filled.Length().ok()? as usize;
    let mut out = vec![0u8; len];
    reader.ReadBytes(&mut out).ok()?;
    Some(out)
}

fn refresh_playback(session: &GlobalSystemMediaTransportControlsSession) {
    let Ok(info) = session.GetPlaybackInfo() else { return };
    let Ok(status) = info.PlaybackStatus() else { return };
    let playing = status == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing;

    let prev = LAST_PLAYING.swap(playing as i8, Ordering::AcqRel);
    let changed = prev != -1 && prev != (playing as i8);

    app::update_playing(playing);

    let controls = info.Controls().ok();
    let shuffle_supported = controls
        .as_ref()
        .and_then(|c| c.IsShuffleEnabled().ok())
        .unwrap_or(false);
    let repeat_supported = controls
        .as_ref()
        .and_then(|c| c.IsRepeatEnabled().ok())
        .unwrap_or(false);

    let shuffle_active = info
        .IsShuffleActive()
        .and_then(|r| r.Value())
        .unwrap_or(false);
    let repeat = info
        .AutoRepeatMode()
        .and_then(|r| r.Value())
        .map(|m| match m {
            MediaPlaybackAutoRepeatMode::Track => app::RepeatMode::Track,
            MediaPlaybackAutoRepeatMode::List => app::RepeatMode::List,
            _ => app::RepeatMode::None,
        })
        .unwrap_or(app::RepeatMode::None);

    app::update_shuffle(shuffle_active, shuffle_supported);
    app::update_repeat(repeat, repeat_supported);

    if changed {
        post_show_if_enabled();
    }
}

fn refresh_timeline(session: &GlobalSystemMediaTransportControlsSession) {
    let Ok(t) = session.GetTimelineProperties() else { return };
    let pos = timespan_to_secs(t.Position().unwrap_or(TimeSpan { Duration: 0 }));
    let end = timespan_to_secs(t.EndTime().unwrap_or(TimeSpan { Duration: 0 }));
    let start = timespan_to_secs(t.StartTime().unwrap_or(TimeSpan { Duration: 0 }));
    let duration = (end - start).max(0.0);

    let seekable = session
        .GetPlaybackInfo()
        .and_then(|info| info.Controls().and_then(|c| c.IsPlaybackPositionEnabled()))
        .unwrap_or(false);
    app::update_timeline(pos, duration, seekable);
}

fn timespan_to_secs(t: TimeSpan) -> f64 {
    // TimeSpan.Duration is in 100ns ticks
    t.Duration as f64 / 10_000_000.0
}

pub fn try_play_pause() {
    if let Some(s) = current_session() {
        let Ok(info) = s.GetPlaybackInfo() else { return };
        let Ok(status) = info.PlaybackStatus() else { return };
        let _ = if status == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing {
            s.TryPauseAsync().and_then(|op| op.get())
        } else {
            s.TryPlayAsync().and_then(|op| op.get())
        };
    }
}

pub fn try_next() {
    if let Some(s) = current_session() {
        if let Ok(op) = s.TrySkipNextAsync() {
            let _ = op.get();
        }
    }
}

pub fn try_prev() {
    if let Some(s) = current_session() {
        if let Ok(op) = s.TrySkipPreviousAsync() {
            let _ = op.get();
        }
    }
}

pub fn try_toggle_shuffle() {
    if let Some(s) = current_session() {
        let active = s
            .GetPlaybackInfo()
            .and_then(|i| i.IsShuffleActive())
            .and_then(|r| r.Value())
            .unwrap_or(false);
        if let Ok(op) = s.TryChangeShuffleActiveAsync(!active) {
            let _ = op.get();
        }
    }
}

pub fn try_cycle_repeat() {
    if let Some(s) = current_session() {
        let current = s
            .GetPlaybackInfo()
            .and_then(|i| i.AutoRepeatMode())
            .and_then(|r| r.Value())
            .unwrap_or(MediaPlaybackAutoRepeatMode::None);
        let next = match current {
            MediaPlaybackAutoRepeatMode::None => MediaPlaybackAutoRepeatMode::List,
            MediaPlaybackAutoRepeatMode::List => MediaPlaybackAutoRepeatMode::Track,
            _ => MediaPlaybackAutoRepeatMode::None,
        };
        if let Ok(op) = s.TryChangeAutoRepeatModeAsync(next) {
            let _ = op.get();
        }
    }
}

pub fn try_seek(seconds: f64) {
    if let Some(s) = current_session() {
        let ticks: i64 = (seconds.max(0.0) * 10_000_000.0) as i64;
        if let Ok(op) = s.TryChangePlaybackPositionAsync(ticks) {
            let _ = op.get();
        }
    }
}

pub fn poll_timeline() {
    if let Some(s) = current_session() {
        refresh_timeline(&s);
    }
}

#[allow(dead_code)]
pub fn poll_throttle() -> Duration {
    Duration::from_millis(300)
}
