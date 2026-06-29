use std::{
    sync::{Arc, Mutex, atomic::Ordering, mpsc},
    time::Duration,
};

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::watch;
use windows::Win32::{
    Media::Audio::{
        AudioSessionState, AudioSessionStateActive, DEVICE_STATE_ACTIVE, DEVICE_STATE_DISABLED,
        DEVICE_STATE_NOTPRESENT, DEVICE_STATE_UNPLUGGED, IAudioSessionControl,
        IAudioSessionControl2, IAudioSessionEvents, IAudioSessionEvents_Impl,
        IAudioSessionManager2, IAudioSessionNotification, IAudioSessionNotification_Impl,
        IMMDeviceEnumerator, IMMNotificationClient, IMMNotificationClient_Impl, MMDeviceEnumerator,
        eCapture, eCommunications,
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
    let (device_change_tx, device_change_rx) = mpsc::sync_channel::<()>(1);
    let notifier: IMMNotificationClient = DeviceChangeNotifier { tx: device_change_tx }.into();

    let enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? };

    unsafe {
        enumerator.RegisterEndpointNotificationCallback(&notifier)?;
    }

    let _guard = NotifierGuard(&enumerator, &notifier);

    let mut active_device_handlers: Vec<SessionHandlers> = Vec::new();
    let (notify_tx, mut notify_rx) = watch::channel(());

    let rebuild_all_handlers = |handlers: &mut Vec<SessionHandlers>| {
        handlers.clear();

        unsafe {
            if let Ok(collection) = enumerator.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE) {
                if let Ok(count) = collection.GetCount() {
                    for i in 0..count {
                        if let Ok(device) = collection.Item(i) {
                            if let Ok(manager) =
                                device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None)
                            {
                                if let Ok(handler) = SessionHandlers::new(manager, &notify_tx) {
                                    handlers.push(handler);
                                }
                            }
                        }
                    }
                }
            }
        }
    };

    rebuild_all_handlers(&mut active_device_handlers);

    let initial_state = evaluate_mic_status(&active_device_handlers);
    service.active = initial_state;
    runtime.mic.store(initial_state, Ordering::Relaxed);

    loop {
        if device_change_rx.try_recv().is_ok() {
            while device_change_rx.try_recv().is_ok() {}
            eprintln!(
                "[MicrophoneService] Hardware layout changed. Re-indexing all microphones..."
            );
            rebuild_all_handlers(&mut active_device_handlers);
        }

        let mut should_evaluate = false;

        match tokio::time::timeout(Duration::from_millis(250), notify_rx.changed()).await {
            Ok(Ok(())) => {
                while notify_rx.has_changed().unwrap_or(false) {
                    notify_rx.mark_unchanged();
                }
                should_evaluate = true;
            }
            _ => {}
        }

        if should_evaluate {
            let found_active = evaluate_mic_status(&active_device_handlers);

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

fn evaluate_mic_status(handlers: &[SessionHandlers]) -> bool {
    for handler in handlers {
        unsafe {
            if let Ok(session_enum) = handler.manager.GetSessionEnumerator() {
                if let Ok(count) = session_enum.GetCount() {
                    for i in 0..count {
                        if let Ok(control) = session_enum.GetSession(i) {
                            if let (Ok(state), Ok(control2)) =
                                (control.GetState(), control.cast::<IAudioSessionControl2>())
                            {
                                if state == AudioSessionStateActive
                                    && control2.GetProcessId().unwrap_or(0) != 0
                                {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    false
}

#[implement(IMMNotificationClient)]
struct DeviceChangeNotifier {
    tx: mpsc::SyncSender<()>,
}

impl IMMNotificationClient_Impl for DeviceChangeNotifier_Impl {
    fn OnDefaultDeviceChanged(
        &self,
        flow: windows::Win32::Media::Audio::EDataFlow,
        role: windows::Win32::Media::Audio::ERole,
        _: &windows_core::PCWSTR,
    ) -> windows_core::Result<()> {
        if flow == eCapture && role == eCommunications {
            let _ = self.tx.try_send(());
        }
        Ok(())
    }

    fn OnDeviceAdded(&self, _: &windows_core::PCWSTR) -> windows_core::Result<()> {
        let _ = self.tx.try_send(());
        Ok(())
    }

    fn OnDeviceRemoved(&self, _: &windows_core::PCWSTR) -> windows_core::Result<()> {
        let _ = self.tx.try_send(());
        Ok(())
    }

    fn OnDeviceStateChanged(
        &self,
        _: &windows_core::PCWSTR,
        dwstate: windows::Win32::Media::Audio::DEVICE_STATE,
    ) -> windows_core::Result<()> {
        if matches!(
            dwstate,
            DEVICE_STATE_ACTIVE
                | DEVICE_STATE_DISABLED
                | DEVICE_STATE_NOTPRESENT
                | DEVICE_STATE_UNPLUGGED
        ) {
            let _ = self.tx.try_send(());
        }
        Ok(())
    }

    fn OnPropertyValueChanged(
        &self,
        _: &windows_core::PCWSTR,
        _: &windows::Win32::Foundation::PROPERTYKEY,
    ) -> windows_core::Result<()> {
        Ok(())
    }
}

struct NotifierGuard<'a>(&'a IMMDeviceEnumerator, &'a IMMNotificationClient);

impl Drop for NotifierGuard<'_> {
    fn drop(&mut self) {
        unsafe {
            let _ = self.0.UnregisterEndpointNotificationCallback(self.1);
        }
    }
}
