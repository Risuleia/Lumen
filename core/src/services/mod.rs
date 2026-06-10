use std::sync::Arc;

use async_trait::async_trait;

use crate::{bus::EventSender, runtime::RuntimeState};

pub mod camera;
pub mod media;
pub mod notifications;
pub mod audio;
pub mod microphone;

#[async_trait]
pub trait Service: Send + Sync {
    fn new() -> Self where Self: Sized;

    async fn run(
        self,
        tx: EventSender,
        runtime: Arc<RuntimeState>
    );
}