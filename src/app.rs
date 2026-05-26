use std::sync::{Mutex, OnceLock};

#[derive(Clone, Debug, Default)]
pub struct TrackInfo {
    pub title: String,
    pub artist: String,
    pub playing: bool,
    pub position_secs: f64,
    pub duration_secs: f64,
    pub thumbnail_key: String,
    pub thumbnail_bytes: Vec<u8>,
    pub has_thumbnail: bool,
    pub seekable: bool,
}

static STATE: OnceLock<Mutex<TrackInfo>> = OnceLock::new();

fn state() -> &'static Mutex<TrackInfo> {
    STATE.get_or_init(|| Mutex::new(TrackInfo::default()))
}

pub fn update_track(title: String, artist: String) {
    if let Ok(mut s) = state().lock() {
        s.title = title;
        s.artist = artist;
    }
}

pub fn update_playing(playing: bool) {
    if let Ok(mut s) = state().lock() {
        s.playing = playing;
    }
}

pub fn update_timeline(position_secs: f64, duration_secs: f64, seekable: bool) {
    if let Ok(mut s) = state().lock() {
        s.position_secs = position_secs;
        s.duration_secs = duration_secs;
        s.seekable = seekable;
    }
}

pub fn update_thumbnail(key: String, bytes: Vec<u8>) {
    if let Ok(mut s) = state().lock() {
        s.thumbnail_key = key;
        s.thumbnail_bytes = bytes;
        s.has_thumbnail = !s.thumbnail_bytes.is_empty();
    }
}

pub fn clear_thumbnail() {
    if let Ok(mut s) = state().lock() {
        s.thumbnail_key.clear();
        s.thumbnail_bytes.clear();
        s.has_thumbnail = false;
    }
}

pub fn snapshot() -> TrackInfo {
    state().lock().map(|s| s.clone()).unwrap_or_default()
}
