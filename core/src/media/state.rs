#[derive(Debug, Clone)]
pub enum PlaybackState {
    Playing,
    Paused,
    Stopped
}

#[derive(Debug, Clone)]
pub enum MediaType {
    Music,
    Video,
    Stream,
    Unknown
}

#[derive(Debug, Clone)]
pub struct MediaState {
    pub app_name: Option<String>,
    pub app_id: Option<String>,
    pub app_icon: Option<Vec<u8>>,
    
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,

    pub duration_ms: Option<u64>,
    pub position_ms: Option<u64>,

    pub playing: bool,
    pub playback_state: PlaybackState,

    pub media_type: MediaType,

    pub artwork: Option<Vec<u8>>
}