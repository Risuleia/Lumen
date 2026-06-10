use std::time::Duration;

use lumen_core::CoreEvent;

use crate::state::{BaseState, IslandState, Notification, OverlayState};

pub enum Event {
    MediaStarted,
    MediaStopped,

    CallStarted,
    CallEnded,

    Notification(Notification),
    NotificationExpired,
}

impl IslandState {
    pub fn transition(&mut self, event: Event) {
        match event {
            // ---- BASE STATE CHANGES ----

            Event::MediaStarted => {
                self.base = BaseState::Media;
            }

            Event::MediaStopped => {
                if self.base == BaseState::Media {
                    self.base = BaseState::Hidden;
                }
            }

            Event::CallStarted => {
                self.base = BaseState::Call;
            }

            Event::CallEnded => {
                if self.base == BaseState::Call {
                    self.base = BaseState::Hidden;
                }
            }

            // ---- OVERLAY LOGIC ----

            Event::Notification(notification) => {
                self.overlay = Some(OverlayState::Notification(notification));
            }

            Event::NotificationExpired => {
                self.overlay = None;
            }
        }
    }

    pub fn dispatch(&mut self, ev: CoreEvent) {
        match ev {
            CoreEvent::AudioSpectrum(spec) => {
                self.spectrum = spec.bands;
            }

            CoreEvent::MediaStarted(info) | CoreEvent::MediaUpdated(info) => {
                self.media = Some(info);
                if self.call.is_none() {
                    self.base = BaseState::Media;
                }
            }

            CoreEvent::MediaStopped => {
                self.media = None;
                if self.call.is_none() {
                    self.base = BaseState::Hidden;
                }
            }

            CoreEvent::NotificationReceived(n) => {
                self.overlay = Some(OverlayState::Notification(Notification {
                    title: n.title,
                    body: n.body,
                    timeout: Duration::from_secs(4)
                }));
            }

            CoreEvent::CallStarted(call) => {
                self.call = Some(call);
                self.base = BaseState::Call;
            }

            CoreEvent::CallEnded => {
                self.call = None;
                if self.media.is_none() {
                    self.base = BaseState::Hidden
                } else {
                    self.base = BaseState::Media
                }
            }
        }
    }
}