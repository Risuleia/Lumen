use std::{cell::RefCell, collections::HashMap, path::Path};

use lumen_core::{MediaState, NotificationState};
use slint::{Image, SharedString};

use crate::{MediaState as SlintMediaState, NotificationState as SlintNotificationState};

thread_local! {
    static LOCAL_TEXTURE_CACHE: RefCell<HashMap<String, Image>> = RefCell::new(HashMap::new());
}

fn load_image(path: Option<&str>, fallback: &Image) -> Image {
    let Some(path_str) = path else {
        return fallback.clone();
    };
    if path_str.is_empty() {
        return fallback.clone();
    }

    let cached_match = LOCAL_TEXTURE_CACHE.with(|cache| cache.borrow().get(path_str).cloned());
    if let Some(cached_image) = cached_match {
        return cached_image;
    }

    if let Ok(image) = Image::load_from_path(Path::new(path_str)) {
        LOCAL_TEXTURE_CACHE.with(|cache| {
            cache.borrow_mut().insert(path_str.to_string(), image.clone());
        });

        return image;
    }

    fallback.clone()
}

pub fn media_to_slint(
    media: &MediaState,
    fallback_app: &Image,
    fallback_album: &Image,
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
