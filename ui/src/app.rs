use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use lumen_core::{IslandCore, RuntimeState};
use slint::{ComponentHandle, Weak};

use crate::{
    Assets, IslandContent, IslandData, Shell,
    state::{ContentState, IslandState},
    sync::{media_to_slint, notification_to_slint},
};

#[derive(Clone)]
pub struct Lumen {
    state: Arc<Mutex<IslandState>>,
    shell: Option<Weak<Shell>>,
    core: Arc<IslandCore>,
}

impl Lumen {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(IslandState::new())),
            shell: None,
            core: Arc::new(IslandCore::new()),
        }
    }

    pub fn start(&mut self, shell: &Shell) -> Result<()> {
        self.attach_shell(shell);
        self.attach_core();

        self.attach_tick();
        self.attach_actions();

        self.core.start();

        self.dispatch();

        if let Some(shell) = self.shell.as_ref().and_then(|s| s.upgrade()) {
            shell.run()?;
            return Ok(());
        }

        Err(anyhow!("No shell attached"))
    }

    fn attach_shell(&mut self, shell: &Shell) {
        self.shell = Some(shell.as_weak());
    }

    fn attach_core(&self) {
        let rx = self.core.subscribe();

        let lumen = self.clone();

        std::thread::spawn(move || {
            while rx.recv().is_ok() {
                let lumen = lumen.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    lumen.dispatch();
                });
            }
        });
    }

    pub fn state(&self) -> Arc<Mutex<IslandState>> {
        self.state.clone()
    }

    pub fn runtime(&self) -> Arc<RuntimeState> {
        self.core.runtime()
    }

    pub fn dispatch(&self) {
        self.refresh_content();
        self.sync_shell();
    }

    fn refresh_content(&self) {
        let runtime = self.runtime();

        self.set_mic(runtime.mic.load(std::sync::atomic::Ordering::Relaxed));
        self.set_camera(runtime.camera.load(std::sync::atomic::Ordering::Relaxed));

        if let Some(notification) = runtime.notifications.lock().unwrap().front().cloned() {
            self.set_content(ContentState::Notification(notification));
            return;
        }

        if let Some(media) = runtime.media.read().unwrap().as_ref().cloned() {
            if media.playing {
                self.set_content(ContentState::Media(media));
                return;
            }
        }

        self.set_content(ContentState::Idle);
    }

    fn sync_shell(&self) {
        if let Some(shell) = &self.shell.as_ref().and_then(|s| s.upgrade()) {
            let state = self.state.lock().unwrap();

            let content = state.content.clone();
            let bounds = state.bounds();
            let mic = state.mic;
            let camera = state.camera;
            let expanded = state.expanded;

            drop(state);

            let global = shell.global::<IslandData>();

            global.set_expanded(expanded);
            global.set_mic(mic);
            global.set_camera(camera);

            let assets = shell.global::<Assets>();

            match &content {
                ContentState::Idle => {
                    global.set_content(IslandContent::Idle);
                }
                ContentState::Media(m) => {
                    global.set_media(media_to_slint(
                        m,
                        &assets.get_fallback_app(),
                        &assets.get_fallback_media(),
                    ));
                    global.set_content(IslandContent::Media);
                }
                ContentState::Notification(n) => {
                    global.set_notification(notification_to_slint(n, &assets.get_fallback_app()));
                    global.set_content(IslandContent::Notification);
                }
            }

            shell.set_island_width(bounds.width as f32);
            shell.set_island_height(bounds.height as f32);
            shell.set_island_radius(bounds.radius as f32);
            shell.set_island_y(bounds.y as f32);
        }
    }

    fn attach_tick(&self) {
        let runtime = self.runtime();

        if let Some(shell) = self.shell.as_ref().and_then(|s| s.upgrade()) {
            let weak = shell.as_weak();

            shell.on_tick(move || {
                let Some(shell) = weak.upgrade() else {
                    return;
                };
                
                let media = { runtime.media.read().unwrap().clone() };

                if let Some(media) = media {
                    if !media.playing { return; }
                    
                    let global = shell.global::<IslandData>();

                    let spectrum = {
                        let spectrum = runtime.spectrum.read().unwrap();
                        (&spectrum[..]).into()
                    };
                    global.set_spectrum(spectrum);

                    global.set_media_position(media.current_position_ms() as i32);
                };
            });
        }
    }

    fn attach_actions(&self) {
        let Some(shell) = self.shell.as_ref().and_then(|s| s.upgrade()) else {
            return;
        };

        let lumen = self.clone();

        shell
            .global::<IslandData>()
            .on_action(move |action, payload| {
                lumen.handle_action(&action, &payload);
            });
    }

    fn handle_action(&self, action: &str, payload: &str) {
        match action {
            "expand" => {
                self.set_expanded(payload == "true");
                self.sync_shell();
            }
            "dismiss-notification" => {
                let Ok(id) = payload.parse::<u64>() else { return; };
                self.core.dismiss_notification(id);
            }
            "toggle-playback" => {
                let core = self.core.clone();
                std::thread::spawn(move || {
                    let _ = futures::executor::block_on(core.toggle_playback());
                });
            }
            "next" => {
                let core = self.core.clone();
                std::thread::spawn(move || {
                    let _ = futures::executor::block_on(core.next());
                });
            }
            "previous" => {
                let core = self.core.clone();
                std::thread::spawn(move || {
                    let _ = futures::executor::block_on(core.previous());
                });
            }
            "seek" => {
                let Ok(position) = payload.parse::<u64>() else { return; };
                let core = self.core.clone();
                std::thread::spawn(move || {
                    let _ = futures::executor::block_on(core.seek(position));
                });
            }

            _ => {
                eprintln!("[Lumen] Unknown action: {action} ({payload})");
            }
        }
    }

    fn set_content(&self, content: ContentState) {
        let mut state = self.state.lock().unwrap();
        state.content = content;
    }

    fn set_mic(&self, active: bool) {
        let mut state = self.state.lock().unwrap();
        state.mic = active;
    }

    fn set_camera(&self, active: bool) {
        let mut state = self.state.lock().unwrap();
        state.camera = active;
    }

    pub fn set_expanded(&self, expanded: bool) {
        let mut state = self.state.lock().unwrap();
        state.expanded = expanded;
    }
}
