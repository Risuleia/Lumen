use crate::{media::{MediaState}, state::IslandState};

#[derive(Debug, Clone)]
pub enum CoreEvent {
    StateChanged(IslandState),
    MediaUpdated(MediaState),
    MediaStopped,
    MicActive(f32),
    MicIdle,
    CameraActive,
    CameraIdle,
    VisualizerFrame(f32)
}