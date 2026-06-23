use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::watch;
use windows::{
    Foundation::TypedEventHandler,
    Media::Control::{
        GlobalSystemMediaTransportControlsSession,
        GlobalSystemMediaTransportControlsSessionManager,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus,
    },
    Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx},
};

use crate::{
    CoreEvent, MediaState,
    bus::EventSender,
    runtime::RuntimeState,
    services::Service,
    utils::{artwork::extract_album_art, icon::resolve_app_icon, name::resolve_name_from_aumid},
};

pub struct MediaService {
    current: Option<MediaState>,
}

#[async_trait]
impl Service for MediaService {
    fn new() -> Self {
        Self { current: None }
    }

    async fn run(mut self, tx: EventSender, runtime: Arc<RuntimeState>) {
        std::thread::spawn(move || {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            }

            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to build media runtime");

            rt.block_on(async move {
                if let Err(e) = run_event_driven(&mut self, tx, runtime).await {
                    eprintln!("[MediaService] Fatal error: {e}");
                }
            });
        });
    }
}

struct SessionHandlers {
    session: GlobalSystemMediaTransportControlsSession,
    media_token: i64,
    playback_token: i64,
    timeline_token: i64
}

impl SessionHandlers {
    fn new(
        session: GlobalSystemMediaTransportControlsSession,
        notify_tx: &watch::Sender<()>,
    ) -> Result<Self> {
        let n = notify_tx.clone();
        let media_token =
            session.MediaPropertiesChanged(&TypedEventHandler::new(move |_, _| {
                let _ = n.send(());
                Ok(())
            }))?;

        let n = notify_tx.clone();
        let playback_token =
            session.PlaybackInfoChanged(&TypedEventHandler::new(move |_, _| {
                let _ = n.send(());
                Ok(())
            }))?;

        let n = notify_tx.clone();
        let timeline_token =
            session.TimelinePropertiesChanged(&TypedEventHandler::new(move |_, _| {
                let _ = n.send(());
                Ok(())
            }))?;

        Ok(Self { session, media_token, playback_token, timeline_token })
    }
}

impl Drop for SessionHandlers {
    fn drop(&mut self) {
        let _ = self.session.RemoveMediaPropertiesChanged(self.media_token);
        let _ = self.session.RemovePlaybackInfoChanged(self.playback_token);
        let _ = self.session.RemoveTimelinePropertiesChanged(self.timeline_token);
    }
}

async fn run_event_driven(
    service: &mut MediaService,
    tx: EventSender,
    runtime: Arc<RuntimeState>,
) -> Result<()> {
    let manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()?.await?;
    let (notify_tx, mut notify_rx) = watch::channel(());

    let ntx = notify_tx.clone();
    manager.CurrentSessionChanged(&TypedEventHandler::new(move |_, _| {
        let _ = ntx.send(());
        Ok(())
    }))?;

    let mut _handlers: Option<SessionHandlers> = None;
    let mut current_session_aumid: Option<String> = None;

    if let Ok(session) = manager.GetCurrentSession() {
        if let Ok(aumid) = session.SourceAppUserModelId() {
            current_session_aumid = Some(aumid.to_string());
        }
        _handlers = SessionHandlers::new(session.clone(), &notify_tx).ok();

        if let Ok(initial_state) = build_media_state(&session).await {
            *runtime.media.write().unwrap() = Some(initial_state.clone());
            service.current = Some(initial_state.clone());
            let _ = tx.send(CoreEvent::MediaStarted(initial_state));
        }
    }

    loop {
        notify_rx.changed().await.ok();

        tokio::time::sleep(Duration::from_millis(50)).await;
        while notify_rx.has_changed().unwrap_or(false) {
            notify_rx.mark_unchanged();
        }

        let session = manager.GetCurrentSession();

        let new_aumid = match &session {
            Ok(s) => s.SourceAppUserModelId().map(|id| id.to_string()).ok(),
            Err(_) => None,
        };

        if new_aumid != current_session_aumid {
            _handlers = None;
            current_session_aumid = new_aumid;
            if let Ok(ref s) = session {
                _handlers = SessionHandlers::new(s.clone(), &notify_tx).ok();
            }
        }

        let new = match &session {
            Ok(s) => {
                match tokio::time::timeout(Duration::from_millis(400), build_media_state(s)).await {
                    Ok(Ok(state)) => Some(state),
                    _ => None,
                }
            }
            Err(_) => None,
        };

        match (&service.current, &new) {
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
        service.current = new;
    }
}

async fn build_media_state(
    session: &GlobalSystemMediaTransportControlsSession,
) -> Result<MediaState> {
    let props = session.TryGetMediaPropertiesAsync()?.await?;
    let playback = session.GetPlaybackInfo()?.PlaybackStatus()?;
    let timeline = session.GetTimelineProperties()?;

    let duration_ms = timeline.EndTime()?.Duration as u64 / 10_000;
    let position_ms = timeline.Position()?.Duration as u64 / 10_000;
    let playing = playback == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing;

    let app_id = session.SourceAppUserModelId()?.to_string();

    let last_updated_filetime = timeline.LastUpdatedTime()?;
    let win32_ticks = last_updated_filetime.UniversalTime;
    let unix_ms = (win32_ticks / 10_000) - 11_644_473_600_000;
    let synced_at = SystemTime::UNIX_EPOCH + Duration::from_millis(unix_ms as u64);

    Ok(MediaState {
        app_name: resolve_name_from_aumid(&app_id),
        title: props.Title()?.to_string(),
        artist: props.Artist()?.to_string(),
        album: props.AlbumTitle()?.to_string(),
        album_art: extract_album_art(&props).await?,
        duration_ms,
        position_ms,
        playing,
        app_icon: resolve_app_icon(&app_id).await,
        synced_at,
    })
}
