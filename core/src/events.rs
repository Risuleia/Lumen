use std::time::SystemTime;

#[derive(Debug, Clone)]
pub enum CoreEvent {
    MediaStarted(MediaState),
    MediaStopped,
    TrackChanged(MediaState),

    NotificationReceived(NotificationState),

    MicrophoneActive,
    MicrophoneInactive,

    CameraActive,
    CameraInactive,

    Arbitrary,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NotificationState {
    pub id: u64,

    pub app_name: String,
    pub app_icon: Option<String>,

    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone)]
pub struct MediaState {
    pub synced_at: SystemTime,

    pub app_name: String,

    pub title: String,
    pub artist: String,
    pub album: String,

    pub album_art: Option<String>,

    pub duration_ms: u64,
    pub position_ms: u64,

    pub playing: bool,

    pub app_icon: Option<String>,
}

impl MediaState {
    pub fn current_position_ms(&self) -> u64 {
        if !self.playing {
            return self.position_ms;
        }

        match SystemTime::now().duration_since(self.synced_at) {
            Ok(elapsed) => {
                let local_elapsed_ms = elapsed.as_millis() as u64;
                (self.position_ms + local_elapsed_ms).min(self.duration_ms)
            }
            Err(_) => self.position_ms,
        }
    }
}

impl PartialEq for MediaState {
    fn eq(&self, other: &Self) -> bool {
        self.app_name == other.app_name
            && self.title == other.title
            && self.artist == other.artist
            && self.album == other.album
            && self.album_art == other.album_art
            && self.duration_ms == other.duration_ms
            && self.playing == other.playing
            && self.app_icon == other.app_icon
            && self.position_ms.abs_diff(other.position_ms) < 15
    }

    fn ne(&self, other: &Self) -> bool {
        !self.eq(other)
    }
}
