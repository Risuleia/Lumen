use anyhow::Result;
use windows::Media::Control::{GlobalSystemMediaTransportControlsSessionManager, GlobalSystemMediaTransportControlsSessionPlaybackStatus};

use crate::media::{art::{get_app_icon_bytes, get_thumbnail_bytes}, state::MediaState};

pub struct MediaManager {
    manager: GlobalSystemMediaTransportControlsSessionManager
}

impl MediaManager {
    pub async fn new() -> Result<Self> {
        let manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()?
            .await?;

        Ok(Self { manager })
    }

    pub async fn get_current_state(&self) -> Result<Option<MediaState>> {
        let session = self.manager.GetCurrentSession()?;

        let playback = session.GetPlaybackInfo()?;
        let status = playback.PlaybackStatus()?;

        let timeline = session.GetTimelineProperties()?;

        let playing = matches!(
            status,
            GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing
        );

        let props = session.TryGetMediaPropertiesAsync()?.await?;

        let title = props.Title().ok().filter(|s| !s.is_empty()).map(|s| s.to_string_lossy());
        let artist = props.Artist().ok().filter(|s| !s.is_empty()).map(|s| s.to_string_lossy());
        let album = props.AlbumTitle().ok().filter(|s| !s.is_empty()).map(|s| s.to_string_lossy());
        let app = session.SourceAppUserModelId().ok().map(|s| s.to_string_lossy());

        let app_icon = get_app_icon_bytes(&session).await?;

        let duration_ms = timeline.EndTime()
            .ok()
            .and_then(|t| Some(t.Duration))
            .map(|d| (d / 10000) as u64);
        
        let position_ms = timeline.Position()
            .ok()
            .and_then(|t| Some(t.Duration))
            .map(|d| (d / 10000) as u64);

        let artwork = get_thumbnail_bytes(&session).await?;

        Ok(Some(MediaState {
            app_name: app.clone(),
            app_id: app,
            app_icon,
            title,
            artist,
            album,

            duration_ms,
            position_ms,

            playing,
            playback_state: if playing {
                crate::media::state::PlaybackState::Playing
            } else {
                crate::media::state::PlaybackState::Paused
            },

            media_type: crate::media::state::MediaType::Unknown,
            artwork
        }))
    }
}