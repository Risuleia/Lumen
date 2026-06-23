use std::{
    sync::{Arc, Mutex, atomic::Ordering},
    time::Duration,
};

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::watch;
use windows::Win32::{
    Media::Audio::{
        AudioSessionState, AudioSessionStateActive, IAudioSessionControl, IAudioSessionControl2,
        IAudioSessionEvents, IAudioSessionEvents_Impl, IAudioSessionManager2,
        IAudioSessionNotification, IAudioSessionNotification_Impl, IMMDeviceEnumerator,
        MMDeviceEnumerator, eCapture, eCommunications,
    },
    System::Com::{CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx},
};
use windows_core::{Interface, Ref, implement};

use crate::{CoreEvent, bus::EventSender, runtime::RuntimeState, services::Service};

pub struct MicrophoneService {
    active: bool,
}

#[async_trait]
impl Service for MicrophoneService {
    fn new() -> Self {
        Self { active: false }
    }

    async fn run(mut self, tx: EventSender, runtime: Arc<RuntimeState>) {
        std::thread::spawn(move || {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            }

            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to build microphone runtime");

            rt.block_on(async move {
                if let Err(e) = run_event_driven(&mut self, tx, runtime).await {
                    eprintln!("[MicrophoneService] Fatal error: {e}");
                }
            })
        });
    }
}

struct SessionHandlers {
    manager: IAudioSessionManager2,
    manager_notifier: IAudioSessionNotification,
    active_handlers: Arc<Mutex<Vec<(IAudioSessionControl, IAudioSessionEvents)>>>,
}

impl SessionHandlers {
    fn new(manager: IAudioSessionManager2, notify_tx: &watch::Sender<()>) -> Result<Self> {
        let active_handlers = Arc::new(Mutex::new(Vec::new()));

        if let Ok(session_enum) = unsafe { manager.GetSessionEnumerator() } {
            if let Ok(count) = unsafe { session_enum.GetCount() } {
                let mut handlers = active_handlers.lock().unwrap();
                for i in 0..count {
                    if let Ok(control) = unsafe { session_enum.GetSession(i) } {
                        let state_notifier: IAudioSessionEvents =
                            MicStateNotifier { tx: notify_tx.clone() }.into();
                        let _ =
                            unsafe { control.RegisterAudioSessionNotification(&state_notifier) };
                        handlers.push((control, state_notifier));
                    }
                }
            }
        }

        let manager_notifier: IAudioSessionNotification =
            MicSessionNotifier { tx: notify_tx.clone(), active_handlers: active_handlers.clone() }
                .into();
        unsafe { manager.RegisterSessionNotification(&manager_notifier)? };

        Ok(Self { manager, manager_notifier, active_handlers })
    }
}

impl Drop for SessionHandlers {
    fn drop(&mut self) {
        unsafe {
            let _ = self.manager.UnregisterSessionNotification(&self.manager_notifier);
        }
        if let Ok(handlers) = self.active_handlers.lock() {
            for (control, state_notifier) in handlers.iter() {
                unsafe {
                    let _ = control.UnregisterAudioSessionNotification(state_notifier);
                }
            }
        }
    }
}

#[implement(IAudioSessionEvents)]
struct MicStateNotifier {
    tx: watch::Sender<()>,
}

impl IAudioSessionEvents_Impl for MicStateNotifier_Impl {
    fn OnStateChanged(&self, _new_state: AudioSessionState) -> windows_core::Result<()> {
        let _ = self.tx.send(());
        Ok(())
    }

    fn OnDisplayNameChanged(
        &self,
        _: &windows_core::PCWSTR,
        _: *const windows_core::GUID,
    ) -> windows_core::Result<()> {
        Ok(())
    }
    fn OnIconPathChanged(
        &self,
        _: &windows_core::PCWSTR,
        _: *const windows_core::GUID,
    ) -> windows_core::Result<()> {
        Ok(())
    }
    fn OnSimpleVolumeChanged(
        &self,
        _: f32,
        _: windows_core::BOOL,
        _: *const windows_core::GUID,
    ) -> windows_core::Result<()> {
        Ok(())
    }
    fn OnChannelVolumeChanged(
        &self,
        _: u32,
        _: *const f32,
        _: u32,
        _: *const windows_core::GUID,
    ) -> windows_core::Result<()> {
        Ok(())
    }
    fn OnGroupingParamChanged(
        &self,
        _: *const windows_core::GUID,
        _: *const windows_core::GUID,
    ) -> windows_core::Result<()> {
        Ok(())
    }
    fn OnSessionDisconnected(
        &self,
        _: windows::Win32::Media::Audio::AudioSessionDisconnectReason,
    ) -> windows_core::Result<()> {
        Ok(())
    }
}

#[implement(IAudioSessionNotification)]
struct MicSessionNotifier {
    tx: watch::Sender<()>,
    active_handlers: Arc<Mutex<Vec<(IAudioSessionControl, IAudioSessionEvents)>>>,
}

impl IAudioSessionNotification_Impl for MicSessionNotifier_Impl {
    fn OnSessionCreated(
        &self,
        new_session: Ref<'_, IAudioSessionControl>,
    ) -> windows_core::Result<()> {
        if let Some(session) = new_session.as_ref() {
            let state_notifier: IAudioSessionEvents =
                MicStateNotifier { tx: self.tx.clone() }.into();
            let _ = unsafe { session.RegisterAudioSessionNotification(&state_notifier) };

            if let Ok(mut handlers) = self.active_handlers.lock() {
                handlers.push((session.clone(), state_notifier));
            }
        }
        let _ = self.tx.send(());
        Ok(())
    }
}

async fn run_event_driven(
    service: &mut MicrophoneService,
    tx: EventSender,
    runtime: Arc<RuntimeState>,
) -> Result<()> {
    let enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? };
    let device = unsafe { enumerator.GetDefaultAudioEndpoint(eCapture, eCommunications)? };
    let manager: IAudioSessionManager2 = unsafe { device.Activate(CLSCTX_ALL, None)? };

    let (notify_tx, mut notify_rx) = watch::channel(());

    let _handlers = SessionHandlers::new(manager.clone(), &notify_tx).ok();

    if let Some(initial_state) = evaluate_mic_status(&manager) {
        service.active = initial_state;
        runtime.mic.store(initial_state, Ordering::Relaxed);
        let _ = tx.send(if initial_state {
            CoreEvent::MicrophoneActive
        } else {
            CoreEvent::MicrophoneInactive
        });
    }

    loop {
        notify_rx.changed().await.ok();

        tokio::time::sleep(Duration::from_millis(100)).await;
        while notify_rx.has_changed().unwrap_or(false) {
            notify_rx.mark_unchanged();
        }

        if let Ok(session_enum) = unsafe { manager.GetSessionEnumerator() } {
            if let Ok(count) = unsafe { session_enum.GetCount() } {
                let mut found_active = false;

                unsafe {
                    for i in 0..count {
                        if let Ok(control) = session_enum.GetSession(i) {
                            if let (Ok(state), Ok(control2)) =
                                (control.GetState(), control.cast::<IAudioSessionControl2>())
                            {
                                if state == AudioSessionStateActive
                                    && control2.GetProcessId().unwrap_or(0) != 0
                                {
                                    found_active = true;
                                    break;
                                }
                            }
                        }
                    }
                }

                if let Some(ref h) = _handlers {
                    if let Ok(mut handlers_lock) = h.active_handlers.lock() {
                        handlers_lock.retain(|(control, state_notifier)| {
                            if unsafe { control.GetState().is_err() } {
                                unsafe {
                                    let _ =
                                        control.UnregisterAudioSessionNotification(state_notifier);
                                }
                                false
                            } else {
                                true
                            }
                        });
                    }
                }

                if found_active != service.active {
                    service.active = found_active;
                    runtime.mic.store(found_active, Ordering::Relaxed);
                    let _ = tx.send(if found_active {
                        CoreEvent::MicrophoneActive
                    } else {
                        CoreEvent::MicrophoneInactive
                    });
                }
            }
        }
    }
}

fn evaluate_mic_status(manager: &IAudioSessionManager2) -> Option<bool> {
    unsafe {
        match manager.GetSessionEnumerator() {
            Ok(session_enum) => {
                let mut found_active = false;
                if let Ok(count) = session_enum.GetCount() {
                    for i in 0..count {
                        if let Ok(control) = session_enum.GetSession(i) {
                            if let (Ok(state), Ok(control2)) =
                                (control.GetState(), control.cast::<IAudioSessionControl2>())
                            {
                                if state == AudioSessionStateActive
                                    && control2.GetProcessId().unwrap_or(0) != 0
                                {
                                    found_active = true;
                                    break;
                                }
                            }
                        }
                    }
                }
                Some(found_active)
            }
            Err(_) => None,
        }
    }
}
