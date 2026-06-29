use anyhow::Result;
use serde::{Deserialize, Serialize};
use windows_core::w;
use windows::Win32::System::Registry::{
    RegCloseKey, RegGetValueW, RegOpenKeyExW, RegSetKeyValueW,
    HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, RRF_RT_REG_DWORD,
};

const SUBKEY: windows::core::PCWSTR =
    w!("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Notifications\\Settings");

const VALUE: windows::core::PCWSTR =
    w!("NOC_GLOBAL_SETTING_TOASTS_ENABLED");

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NotificationConfig {
    pub timeout_ms: u64,
    pub suppress_native_toasts: bool,
}

impl NotificationConfig {
    pub fn sanitize(&mut self) {
        self.timeout_ms = self.timeout_ms.clamp(500, 10_000);
    }
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self { timeout_ms: 3000, suppress_native_toasts: false }
    }
}

pub struct ToastSuppression {
    original: Option<u32>,
    active: bool
}

impl ToastSuppression {
    pub fn new() -> Self {
        Self {
            original: None,
            active: false,
        }
    }

    pub fn sync(&mut self, enabled: bool) -> Result<()> {
        if enabled {
            self.enable()
        } else {
            self.disable()
        }
    }

    fn enable(&mut self) -> Result<()> {
        if self.active {
            return Ok(());
        }

        let current = read_value()?;

        self.original = Some(current);

        if current != 0 {
            write_value(0)?;
        }

        self.active = true;

        Ok(())
    }

    fn disable(&mut self) -> Result<()> {
        if !self.active {
            return Ok(());
        }

        if let Some(original) = self.original.take() {
            let current = read_value()?;

            if current != original {
                write_value(original)?;
            }
        }

        self.active = false;

        Ok(())
    }
}

impl Drop for ToastSuppression {
    fn drop(&mut self) {
        let _ = self.disable();
    }
}

fn read_value() -> Result<u32> {
    unsafe {
        let mut key = HKEY::default();

        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            SUBKEY,
            Some(0),
            KEY_QUERY_VALUE,
            &mut key,
        )
        .ok()?;

        let mut value: u32 = 1;
        let mut size = std::mem::size_of::<u32>() as u32;

        let result = RegGetValueW(
            key,
            None,
            VALUE,
            RRF_RT_REG_DWORD,
            None,
            Some((&mut value as *mut u32).cast()),
            Some(&mut size),
        );

        let _ = RegCloseKey(key);

        result.ok()?;

        Ok(value)
    }
}

fn write_value(value: u32) -> Result<()> {
    unsafe {
        Ok(RegSetKeyValueW(
            HKEY_CURRENT_USER,
            SUBKEY,
            VALUE,
            windows::Win32::System::Registry::REG_DWORD.0,
            Some((&value as *const u32).cast()),
            std::mem::size_of::<u32>() as u32,
        )
        .ok()?)
    }
}