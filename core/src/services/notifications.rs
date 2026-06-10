use std::{collections::HashSet, sync::Arc, time::Duration};

use async_trait::async_trait;
use windows::{UI::Notifications::{Management::{UserNotificationListener, UserNotificationListenerAccessStatus}, NotificationKinds}, Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx}};

use crate::{CoreEvent, NotificationState, bus::EventSender, runtime::RuntimeState, services::Service, utils::icon::resolve_app_icon};

pub struct NotificationService {
    seen: HashSet<u32>
}

#[async_trait]
impl Service for NotificationService {
    fn new() -> Self {
        Self { seen: HashSet::new() }
    }

    async fn run(
        mut self,
        tx: EventSender,
        runtime: Arc<RuntimeState>
    ) {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }

        let listener = UserNotificationListener::Current().unwrap();

        let access = listener
            .RequestAccessAsync()
            .unwrap()
            .await
            .unwrap();

        if access != UserNotificationListenerAccessStatus::Allowed {
            eprintln!("Notification access denied");
            return;
        }

        loop {
            let notifications = listener
                .GetNotificationsAsync(NotificationKinds::Toast)
                .unwrap()
                .await
                .unwrap();

            for notification in notifications {
                let id = notification.Id().unwrap();

                if self.seen.contains(&id) {
                    continue;
                }

                self.seen.insert(id);

                let app_name = notification
                    .AppInfo()
                    .ok()
                    .and_then(|a| a.DisplayInfo().ok())
                    .and_then(|d| d.DisplayName().ok())
                    .map(|s| s.to_string())
                    .unwrap_or_default();

                let app_id = notification
                    .AppInfo()
                    .ok()
                    .and_then(|a| a.AppUserModelId().ok())
                    .map(|s| s.to_string())
                    .unwrap_or_default();

                let mut title = String::new();
                let mut body = String::new();

                if let Ok(toast) = notification.Notification() {
                    if let Ok(visual) = toast.Visual() {
                        if let Ok(bindings) = visual.Bindings() {
                            if let Some(binding) = bindings.into_iter().next() {
                                if let Ok(texts) = binding.GetTextElements() {
                                    let mut texts = texts.into_iter();

                                    if let Some(t) = texts.next() {
                                        title = t.Text().unwrap_or_default().to_string();
                                    }

                                    if let Some(t) = texts.next() {
                                        body = t.Text().unwrap_or_default().to_string();
                                    }
                                }
                            }
                        }
                    }
                }

                let state = NotificationState {
                    id: runtime.notifications.lock().unwrap().len() as u64,
                    app_name,
                    title,
                    body,
                    image: resolve_app_icon(&app_id).ok().flatten(),
                };

                runtime
                    .notifications
                    .lock()
                    .unwrap()
                    .push_back(state.clone());

                let _ = tx.send(CoreEvent::NotificationReceived(state.clone()));

                let runtime = runtime.clone();
                let tx = tx.clone();

                let id = state.id;

                std::thread::spawn(move || {
                    std::thread::sleep(
                        Duration::from_secs(4)
                    );

                    runtime
                        .notifications
                        .lock()
                        .unwrap()
                        .retain(|n| n.id != id);

                    let _ = tx.send(
                        CoreEvent::Arbitrary
                    );
                });
            }

            std::thread::sleep(Duration::from_millis(500));
        }
    }
}