use std::{thread, time::Duration};
use tokio::sync::broadcast;

use crate::CoreEvent;
use windows::Win32::Media::MediaFoundation::*;

pub fn start_camera_thread(tx: broadcast::Sender<CoreEvent>) {
    thread::spawn(move || {
        unsafe {
            MFStartup(MF_VERSION, MFSTARTUP_LITE).ok();
        }

        let mut last_state = false;
        let mut idle_count = 0;

        loop {
            let active = unsafe { is_camera_in_use() };

            if active && !last_state {
                last_state = true;
                idle_count = 0;
                let _ = tx.send(CoreEvent::CameraActive);
            }

            if !active {
                idle_count += 1;

                if idle_count > 10 && last_state {
                    last_state = false;
                    let _ = tx.send(CoreEvent::CameraIdle);
                }
            }

            thread::sleep(Duration::from_millis(300));
        }
    });
}

unsafe fn is_camera_in_use() -> bool {
    let mut attrs: Option<IMFAttributes> = None;

    // Create attribute bag
    if unsafe { MFCreateAttributes(&mut attrs, 1).is_err() } {
        return false;
    }

    let attrs = attrs.unwrap();

    // Select only video capture devices
    if unsafe { attrs.SetGUID(
        &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
        &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID
    ).is_err() } {
        return false;
    }

    let mut devices: *mut Option<IMFActivate> = std::ptr::null_mut();
    let mut count = 0u32;

    // Enumerate camera devices
    if unsafe { MFEnumDeviceSources(&attrs, &mut devices, &mut count).is_err() } || count == 0 {
        return false;
    }

    // Walk devices
    for i in 0..count {
        // SAFETY: devices is an array of Option<IMFActivate>
        let dev_opt = unsafe { (*devices.add(i as usize)).take() };
        if let Some(dev) = dev_opt {
            // Try to activate camera
            let result = unsafe { dev.ActivateObject::<IMFMediaSource>() };

            // If activation FAILS → camera is already in use
            if result.is_err() {
                return true;
            }
        }
    }

    false
}