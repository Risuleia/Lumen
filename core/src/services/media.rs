use std::{sync::Arc, time::{Duration, Instant}};

use anyhow::Result;
use async_trait::async_trait;
use windows::{Media::Control::{GlobalSystemMediaTransportControlsSession, GlobalSystemMediaTransportControlsSessionManager, GlobalSystemMediaTransportControlsSessionPlaybackStatus}, Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx}};

use crate::{CoreEvent, MediaState, bus::EventSender, runtime::RuntimeState, services::Service, utils::{artwork::extract_album_art, icon::resolve_app_icon}};

pub struct MediaService {
    current: Option<MediaState>,
}

#[async_trait]
impl Service for MediaService {
    fn new() -> Self {
        Self { current: None }
    }

    async fn run(
        mut self,
        tx: EventSender,
        runtime: Arc<RuntimeState>
    ) {
        
        loop {
            let result = tokio::task::spawn_blocking(move || {
                unsafe {
                    let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
                }
                
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("failed to build local runtime")
                    .block_on(current_media())
            })
            .await;

            match result {
                Ok(Ok(new)) => {
                    match (&self.current, &new) {
                        (None, Some(media)) => {
                            let _ = tx.send(CoreEvent::MediaStarted(media.clone()));
                        }
                        (Some(_), None) => {
                            let _ = tx.send(CoreEvent::MediaStopped);
                        }
                        (Some(old), Some(new)) => {
                            if old != new {
                                let _ = tx.send(CoreEvent::TrackChanged(new.clone()));
                            }
                        }
                        _ => {}
                    }
        
                    *runtime.media.write().unwrap() = new.clone();
                    
                    self.current = new;
                },
                Ok(Err(e)) => eprintln!("[MediaService] {e}"),
                Err(e) => eprintln!("[MediaService] blocking task panicked: {e}"),
            }

            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }
}

async fn current_media() -> Result<Option<MediaState>> {
    let manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()
        .unwrap()
        .await
        .unwrap();

    let Ok(session) = manager.GetCurrentSession() else {
        return Ok(None);
    };

    build_media_state(&session).await.map(Some)
}

async fn build_media_state(
    session: &GlobalSystemMediaTransportControlsSession
) -> Result<MediaState> {
    let props = session
        .TryGetMediaPropertiesAsync()?
        .await?;

    let playback = session
        .GetPlaybackInfo()?
        .PlaybackStatus()?;

    let timeline = session.GetTimelineProperties()?;

    let duration_ms = timeline.EndTime()?.Duration as u64 / 10_000;
    let position_ms = timeline.Position()?.Duration as u64 / 10_000;

    let playing = 
        playback == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing;

    let app_name = session
        .SourceAppUserModelId()?
        .to_string();

    let app_icon = resolve_app_icon(&app_name.clone()).ok().flatten();

    let synced_at = Instant::now();

    Ok(MediaState {
        app_name,

        title: props.Title()?.to_string(),
        artist: props.Artist()?.to_string(),
        album: props.AlbumTitle()?.to_string(),

        album_art: extract_album_art(&props).await?,

        duration_ms,
        position_ms,

        playing,

        app_icon,
        
        synced_at
    })
}