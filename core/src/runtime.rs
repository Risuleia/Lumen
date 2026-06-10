use std::{collections::VecDeque, sync::{Arc, Mutex, RwLock, atomic::AtomicBool}};

use crate::{MediaState, NotificationState};

pub struct RuntimeState {
    pub media: Arc<RwLock<Option<MediaState>>>,
    pub notifications: Arc<Mutex<VecDeque<NotificationState>>>,

    pub mic: AtomicBool,
    pub camera: AtomicBool,

    pub spectrum: Arc<RwLock<[f32; 24]>>
}

impl RuntimeState {
    pub fn new() -> Self {
        Self {
            media: Arc::new(RwLock::new(None)),
            notifications: Arc::new(Mutex::new(VecDeque::new())),
            mic: AtomicBool::new(false),
            camera: AtomicBool::new(false),
            spectrum: Arc::new(RwLock::new([0.0; 24]))
        }
    }
}