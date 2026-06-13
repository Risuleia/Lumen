use std::{collections::HashSet, sync::Arc, time::Duration};

use async_trait::async_trait;
use windows::{
    UI::Notifications::{
        Management::{UserNotificationListener, UserNotificationListenerAccessStatus}, NotificationKinds, UserNotification
    },
    Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx},
};

use crate::{
    CoreEvent, NotificationState,
    bus::EventSender,
    runtime::RuntimeState,
    services::Service,
    utils::{icon::resolve_app_icon, name::resolve_name_from_aumid},
};

pub struct NotificationService {
    seen: HashSet<u32>,
    initialized: bool,
}

#[async_trait]
impl Service for NotificationService {
    fn new() -> Self {
        Self {
            seen: HashSet::new(),
            initialized: false,
        }
    }

    async fn run(mut self, tx: EventSender, runtime: Arc<RuntimeState>) {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }

        let listener = UserNotificationListener::Current().unwrap();

        let access = listener.RequestAccessAsync().unwrap().await.unwrap();

        if access != UserNotificationListenerAccessStatus::Allowed {
            eprintln!("Notification access denied");
            return;
        }

        loop {
            let mut notifications_to_process = Vec::new();

            {
                let notifications = listener
                    .GetNotificationsAsync(NotificationKinds::Toast)
                    .unwrap()
                    .await
                    .unwrap();

                let mut live_ids: HashSet<u32> = HashSet::new();
                let count = notifications.Size().unwrap_or(0);
                for i in 0..count {
                    if let Ok(n) = notifications.GetAt(i) {
                        if let Ok(id) = n.Id() {
                            live_ids.insert(id);
                        }
                    }
                }

                self.seen.retain(|id| live_ids.contains(id));

                for notification in notifications {
                    let id = notification.Id().unwrap();

                    if self.seen.contains(&id) {
                        continue;
                    }

                    self.seen.insert(id);

                    if !self.initialized {
                        continue;
                    }

                    let app_id = notification
                        .AppInfo()
                        .ok()
                        .and_then(|a| a.AppUserModelId().ok())
                        .map(|s| s.to_string())
                        .unwrap_or_default();

                    let (title, body) = parse_notification(notification);

                    notifications_to_process.push((id, app_id, title, body));
                }
            }

            for (id, app_id, title, body) in notifications_to_process {
                let app_id_clone = app_id.clone();
                let app_icon = tokio::task::spawn_blocking(move || {
                    tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .unwrap()
                        .block_on(resolve_app_icon(&app_id_clone))
                })
                .await
                .unwrap_or(None);

                let state = NotificationState {
                    id: id as u64,
                    app_name: resolve_name_from_aumid(&app_id),
                    app_icon: app_icon,
                    title,
                    body,
                };

                runtime
                    .notifications
                    .lock()
                    .unwrap()
                    .push_back(state.clone());

                let _ = tx.send(CoreEvent::NotificationReceived(state));
            }

            self.initialized = true;

            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }
}

fn parse_notification(notification: UserNotification) -> (String, String) {
    let mut title = String::new();
    let mut body = Vec::new();

    if let Ok(toast) = notification.Notification() {
        if let Ok(visual) = toast.Visual() {
            if let Ok(bindings) = visual.Bindings() {
                if let Some(binding) = bindings.into_iter().next() {
                    if let Ok(texts) = binding.GetTextElements() {
                        for (idx, text) in texts.into_iter().enumerate() {
                            if let Ok(win_str) = text.Text() {
                                let text_content = win_str.to_string();
                                if idx == 0 {
                                    title = text_content;
                                } else if !text_content.is_empty() {
                                    body.push(text_content);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let joined_body = body.join("\n");

    (title, joined_body)
}