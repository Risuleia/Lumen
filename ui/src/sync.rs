use std::path::Path;

use lumen_core::{MediaState, NotificationState};
use slint::{Image, SharedString};

use crate::{MediaState as SlintMediaState, NotificationState as SlintNotificationState};

fn load_image(path: Option<&str>, fallback: &Image) -> Image {
    if let Some(path) = path {
        if let Ok(image) = Image::load_from_path(&Path::new(path)) {
            return image;
        }
    }

    fallback.clone()
}

pub fn media_to_slint(
    media: &MediaState,
    fallback_app: &Image,
    fallback_album: &Image
) -> SlintMediaState {
    SlintMediaState {
        app_name: SharedString::from(&media.app_name),
        app_icon: load_image(media.app_icon.as_deref(), fallback_app),
        
        title: SharedString::from(&media.title),
        album: SharedString::from(&media.album),
        artist: SharedString::from(&media.artist),

        album_art: load_image(media.album_art.as_deref(), fallback_album),

        playing: media.playing,

        duration_ms: media.duration_ms as i32,
    }
}

pub fn notification_to_slint(
    notif: &NotificationState,
    fallback_app: &Image,
) -> SlintNotificationState {
    SlintNotificationState {
        id: SharedString::from(notif.id.to_string()),

        app_name: SharedString::from(&notif.app_name),
        app_icon: load_image(notif.app_icon.as_deref(), fallback_app),
        
        title: SharedString::from(&notif.title),
        body: SharedString::from(&notif.body),
    }
}