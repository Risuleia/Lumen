use std::sync::Arc;

use anyhow::{Result, bail};
use windows::{
    Data::Xml::Dom::XmlDocument,
    Foundation::TypedEventHandler,
    UI::Notifications::{ToastActivatedEventArgs, ToastNotification, ToastNotificationManager},
};
use windows_core::{HSTRING, Interface};

use crate::AUMID;

pub fn show_update_toast(version: &str, on_update: impl Fn() + Send + Sync + 'static) {
    let mut xml = String::with_capacity(512);
    xml.push_str("<toast launch=\"action=update\" activationType=\"foreground\"><visual><binding template=\"ToastGeneric\"><text>Lumen ");
    xml.push_str(version);
    xml.push_str(" is available</text><text>A new version of Lumen is ready to install.</text></binding></visual><actions><action content=\"Update Now\" arguments=\"action=update\" activationType=\"foreground\"/><action content=\"Later\" arguments=\"action=dismiss\" activationType=\"foreground\"/></actions></toast>");

    let shared_callback = Arc::new(on_update);

    if let Err(e) = send_toast(&xml, shared_callback) {
        eprintln!("[Updater] Toast failed to deliver: {e}");
    }
}

fn send_toast(xml: &str, on_update: Arc<impl Fn() + Send + Sync + 'static>) -> Result<()> {
    let doc = XmlDocument::new()?;
    doc.LoadXml(&HSTRING::from(xml))?;

    let toast = ToastNotification::CreateToastNotification(&doc)?;
    let callback_clone = on_update.clone();

    toast.Activated(&TypedEventHandler::new(
        move |_, args: windows_core::Ref<'_, windows_core::IInspectable>| {
            if let Some(toast_args) =
                args.as_ref().and_then(|a| a.cast::<ToastActivatedEventArgs>().ok())
            {
                if let Ok(arguments) = toast_args.Arguments() {
                    if arguments == "action=update" {
                        let final_callback = callback_clone.clone();

                        let _ = slint::invoke_from_event_loop(move || {
                            final_callback();
                        });
                    }
                }
            }
            Ok(())
        },
    ))?;

    let hstring_aumid = HSTRING::from(AUMID);

    let notifier = match ToastNotificationManager::CreateToastNotifierWithId(&hstring_aumid) {
        Ok(n) => n,
        Err(e) => {
            bail!("Windows notification service registry missing: {e}");
        }
    };

    notifier.Show(&toast)?;
    Ok(())
}
