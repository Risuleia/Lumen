use tokio::sync::broadcast;

use crate::{
    ActiveMode, audio::{start_loopback_thread, start_mic_thread}, events::CoreEvent, sources::start_camera_thread, state::{ActivityKind, IslandState, PrivacyKind}
};

pub struct IslandCore {
    state: IslandState,
    pub tx: broadcast::Sender<CoreEvent>,

    current_mode: ActiveMode,
    cooldown_frames: u32,
}

impl IslandCore {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(128);

        Self {
            state: IslandState::IdleDormant,
            tx,
            current_mode: ActiveMode::Idle,
            cooldown_frames: 0,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<CoreEvent> {
        self.tx.subscribe()
    }

    fn set_state(&mut self, state: IslandState) {
        println!("{:?}", state);
        self.state = state.clone();
        let _ = self.tx.send(CoreEvent::StateChanged(state));
    }

    pub async fn start(&mut self) {
        start_loopback_thread(self.tx.clone());
        start_mic_thread(self.tx.clone());
        start_camera_thread(self.tx.clone());

        let mut rx = self.subscribe();

        while let Ok(event) = rx.recv().await {
            println!("{:?}", event);

            match event {
                CoreEvent::CameraActive => {
                    self.update_mode(ActiveMode::Camera);
                }

                CoreEvent::CameraIdle => {
                    self.update_mode(ActiveMode::Idle);
                }

                CoreEvent::MicActive(_) => {
                    self.update_mode(ActiveMode::Mic);
                }
                CoreEvent::MicIdle => {
                    self.update_mode(ActiveMode::Idle);
                }

                CoreEvent::MediaUpdated(_) => {
                    self.update_mode(ActiveMode::Media);
                }
                CoreEvent::MediaStopped => {
                    self.update_mode(ActiveMode::Idle);
                }

                _ => {}
            }
        }
    }

    fn update_mode(&mut self, new: ActiveMode) {
        let priority = |m: ActiveMode| match m {
            ActiveMode::Camera => 3,
            ActiveMode::Mic => 2,
            ActiveMode::Media => 1,
            ActiveMode::Idle => 0,
        };

        let old = self.current_mode;

        if priority(new) < priority(old) {
            if self.cooldown_frames > 10 {
                self.cooldown_frames = 0;
            } else {
                self.cooldown_frames += 1;
                return;
            }
        }

        if new == old {
            return;
        }

        self.current_mode = new;

        match new {
            ActiveMode::Camera => self.set_state(IslandState::PrivacyIndicator(PrivacyKind::Camera)),
            ActiveMode::Mic => self.set_state(IslandState::PrivacyIndicator(PrivacyKind::Microphone)),
            ActiveMode::Media => self.set_state(IslandState::ActiveWidget(ActivityKind::Media)),
            ActiveMode::Idle => self.set_state(IslandState::IdleDormant),
        };
    }
}
