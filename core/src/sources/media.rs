// use std::time::Duration;

// use tokio::sync::broadcast;
// use windows::Media::Control::{GlobalSystemMediaTransportControlsSessionManager, GlobalSystemMediaTransportControlsSessionPlaybackStatus};

// use crate::CoreEvent;

// pub struct MediaWatcher {
//     tx: broadcast::Sender<CoreEvent>,
// }

// impl MediaWatcher {
//     pub async fn run(mut self) {
//         let mgr = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()
//             .unwrap()
//             .get()
//             .unwrap();
        
//         loop {
//             if let Ok(session) = mgr.GetCurrentSession() {
//                 let info = session.GetPlaybackInfo().unwrap();
//                 let status = info.PlaybackStatus().unwrap();
                
//                 match status {
//                     GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing => {
//                         let _ = self.tx.send(CoreEvent::MediaActive);
//                     }
//                     _ => {
//                         let _ = self.tx.send(CoreEvent::MediaStopped);
//                     }
//                 }
//             }

//             tokio::time::sleep(Duration::from_millis(300)).await;
//         }
//     }
// }