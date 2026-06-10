use std::sync::Arc;

use crate::{bus::{EventReceiver, EventSender, create_bus}, runtime::RuntimeState, services::{Service, audio::AudioSpectrumService, camera::CameraService, media::MediaService, microphone::MicrophoneService, notifications::NotificationService}, utils::cache_dir};

pub struct IslandCore {
    tx: EventSender,
    rx: EventReceiver,
    runtime: Arc<RuntimeState>,
    executor: tokio::runtime::Runtime
}

impl IslandCore {
    pub fn new() -> Self {
        let (tx, rx) = create_bus();

        let _ = std::fs::create_dir_all(cache_dir());

        Self {
            tx,
            rx,
            runtime: Arc::new(RuntimeState::new()),
            executor: tokio::runtime::Runtime::new().unwrap()
        }
    }

    pub fn subscribe(&self) -> EventReceiver {
        self.rx.clone()
    }

    pub fn runtime(&self) -> Arc<RuntimeState> {
        self.runtime.clone()
    }

    pub fn sender(&self) -> EventSender {
        self.tx.clone()
    }

    pub fn start(&self) {
        let runtime = self.runtime.clone();
        let tx = self.tx.clone();

        let handle = &self.executor.handle();

        run_service::<MediaService>(handle, tx.clone(), runtime.clone());
        run_service::<NotificationService>(handle, tx.clone(), runtime.clone());
        run_service::<CameraService>(handle, tx.clone(), runtime.clone());
        run_service::<MicrophoneService>(handle, tx.clone(), runtime.clone());
        run_service::<AudioSpectrumService>(handle, tx.clone(), runtime.clone());
    }
}

fn run_service<S: Service>(
    handle: &tokio::runtime::Handle,
    tx: EventSender, 
    runtime: Arc<RuntimeState>, 
) {
    handle.spawn(async move {
        S::new().run(tx, runtime).await;
    });
}