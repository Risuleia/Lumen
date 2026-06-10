use std::time::Instant;

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

    Arbitrary
}

#[derive(Debug, Clone)]
pub struct NotificationState {
    pub id: u64,

    pub app_name: String,
    pub title: String,
    pub body: String,
    pub image: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MediaState {
    pub app_name: String,

    pub title: String,
    pub artist: String,
    pub album: String,

    pub album_art: Option<String>,
    
    pub duration_ms: u64,
    pub position_ms: u64,
    
    pub playing: bool,

    pub app_icon: Option<String>,

    pub synced_at: Instant
}

impl MediaState {
    pub fn current_position_ms(&self) -> u64 {
        if self.playing {
            self.position_ms + self.synced_at.elapsed().as_millis() as u64
        } else {
            self.position_ms
        }
    }
}