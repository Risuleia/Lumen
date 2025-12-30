use crate::media::manager::MediaManager;
use crate::CoreEvent;
use tokio::sync::broadcast;

pub async fn start_media_thread(tx: broadcast::Sender<CoreEvent>) -> anyhow::Result<()> {
    let mgr = MediaManager::new().await?;

    let mut last_playing = false;

    loop {
        match mgr.get_current_state().await {
            Ok(Some(state)) => {
                if state.playing {
                    last_playing = true;
                    let _ = tx.send(CoreEvent::MediaUpdated(state));
                } else {
                    if last_playing {
                        last_playing = false;
                        let _ = tx.send(CoreEvent::MediaStopped);
                    }
                }
            }

            _ => {
                if last_playing {
                    last_playing = false;
                    let _ = tx.send(CoreEvent::MediaStopped);
                }
            }
        }

        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    }
}
