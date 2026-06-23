use std::{collections::HashSet, sync::Arc, time::Duration};

use anyhow::{Result, bail};
use async_trait::async_trait;
use windows::{
    UI::Notifications::{
        Management::{UserNotificationListener, UserNotificationListenerAccessStatus},
        NotificationKinds, UserNotification,
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

pub struct NotificationService;

#[async_trait]
impl Service for NotificationService {
    fn new() -> Self {
        Self
    }

    async fn run(self, tx: EventSender, runtime: Arc<RuntimeState>) {
        std::thread::spawn(move || {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            }

            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to build notification runtime");

            rt.block_on(async move {
                let mut seen_cache = HashSet::new();
                if let Err(e) = run_unpackaged_polling(&mut seen_cache, tx, runtime).await {
                    eprintln!("[NotificationService] Fatal error: {e}");
                }
            });
        });
    }
}

async fn run_unpackaged_polling(
    seen: &mut HashSet<u32>,
    tx: EventSender,
    runtime: Arc<RuntimeState>,
) -> Result<()> {
    let listener = create_listener().await?;

    populate_seen(seen, &listener).await;

    let mut last_known_count = seen.len();

    loop {
        tokio::time::sleep(Duration::from_millis(300)).await;

        let Ok(op) = listener.GetNotificationsAsync(NotificationKinds::Toast) else {
            continue;
        };
        let Ok(notifications) = op.await else {
            continue;
        };

        let current_count = notifications.Size().unwrap_or(0) as usize;

        if current_count == last_known_count {
            continue;
        }

        let mut live_ids = HashSet::with_capacity(current_count);

        for i in 0..current_count as u32 {
            let Ok(notification) = notifications.GetAt(i) else {
                continue;
            };
            let Ok(id) = notification.Id() else {
                continue;
            };

            live_ids.insert(id);

            if seen.contains(&id) {
                continue;
            }
            seen.insert(id);

            let app_id = notification
                .AppInfo()
                .ok()
                .and_then(|a| a.AppUserModelId().ok())
                .map(|s| s.to_string())
                .unwrap_or_default();

            let (title, body) = parse_notification(notification);
            let app_icon = resolve_app_icon(&app_id).await;

            let state = NotificationState {
                id: id as u64,
                app_name: resolve_name_from_aumid(&app_id),
                app_icon,
                title,
                body,
            };

            runtime.notifications.lock().unwrap().push_back(state.clone());
            let _ = tx.send(CoreEvent::NotificationReceived(state));
        }

        seen.retain(|id| live_ids.contains(id));

        last_known_count = seen.len();
    }
}

async fn create_listener() -> Result<UserNotificationListener> {
    let listener = UserNotificationListener::Current()?;
    let access = listener.RequestAccessAsync()?.await?;

    if access != UserNotificationListenerAccessStatus::Allowed {
        eprintln!("Notification access denied");
        bail!("Notification access denied");
    }

    Ok(listener)
}

async fn populate_seen(seen: &mut HashSet<u32>, listener: &UserNotificationListener) {
    let Ok(op) = listener.GetNotificationsAsync(NotificationKinds::Toast) else {
        return;
    };
    let Ok(notifications) = op.await else {
        return;
    };

    let count = notifications.Size().unwrap_or(0);
    for i in 0..count {
        if let Ok(n) = notifications.GetAt(i) {
            if let Ok(id) = n.Id() {
                seen.insert(id);
            }
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

    (title, body.join("\n"))
}
