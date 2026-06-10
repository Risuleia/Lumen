use crossbeam_channel::{Receiver, Sender};

use crate::events::CoreEvent;

pub type EventSender = Sender<CoreEvent>;
pub type EventReceiver = Receiver<CoreEvent>;

pub fn create_bus() -> (EventSender, EventReceiver) {
    crossbeam_channel::unbounded::<CoreEvent>()
}