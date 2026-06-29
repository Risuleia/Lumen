use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
};

use anyhow::Result;
use windows::{
    ApplicationModel::AppInfo,
    Win32::{
        Foundation::PROPERTYKEY,
        System::Com::CoTaskMemFree,
        UI::Shell::{IShellItem2, SHCreateItemFromParsingName, SHLoadIndirectString},
    },
};
use windows_core::{HSTRING, PCWSTR};
use winreg::HKLM;

static NAME_CACHE: LazyLock<Mutex<HashMap<String, String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

const PKEY_SOFTWARE_PRODUCTNAME: PROPERTYKEY = PROPERTYKEY {
    fmtid: windows::core::GUID::from_u128(0x0CEF7D53_FA64_11D1_A203_0000F81FEDEE),
    pid: 7,
};

pub fn resolve_name_from_aumid(aumid: &str) -> String {
    if let Some(name) = NAME_CACHE.lock().unwrap().get(aumid) {
        return name.clone();
    }

    let resolved_name = if let Ok(name) = get_display_name(aumid) {
        name
    } else if let Ok(name) = get_win32_app_name(aumid) {
        name
    } else if let Ok(name) = get_name_from_registry(aumid) {
        name
    } else {
        if aumid.to_lowercase().ends_with(".exe") {
            let name_without_ext = &aumid[..aumid.len() - 4];
            let mut chars = name_without_ext.chars();
            match chars.next() {
                Some(f) => f.to_uppercase().collect::<String>() + chars.as_str(),
                None => aumid.to_string(),
            }
        } else {
            aumid.split('.').collect::<Vec<_>>().join(" ")
        }
    };

    NAME_CACHE.lock().unwrap().insert(aumid.to_string(), resolved_name.clone());
    resolved_name
}

fn get_display_name(aumid: &str) -> Result<String> {
    let aumid_hstring = HSTRING::from(aumid);

    let app_info = AppInfo::GetFromAppUserModelId(&aumid_hstring)?;
    let display_info = app_info.DisplayInfo()?;
    let name = display_info.DisplayName()?.to_string();

    if name.is_empty() {
        anyhow::bail!("Empty UWP display name");
    }
    Ok(name)
}

fn get_win32_app_name(aumid: &str) -> Result<String> {
    let path = format!("shell:AppsFolder\\{aumid}");
    let path_hstring = HSTRING::from(&path);

    unsafe {
        let shell_item: IShellItem2 =
            SHCreateItemFromParsingName(PCWSTR(path_hstring.as_ptr()), None)?;

        if let Ok(name) = shell_item.GetString(&PKEY_SOFTWARE_PRODUCTNAME) {
            let s = name.to_string().unwrap_or_default();
            CoTaskMemFree(Some(name.as_ptr() as *const _));
            if !s.is_empty() {
                return Ok(s);
            }
        }

        let display_pwstr = shell_item
            .GetString(&windows::Win32::Storage::EnhancedStorage::PKEY_ItemNameDisplay)?;
        let display_name = display_pwstr.to_string().unwrap_or_default();
        CoTaskMemFree(Some(display_pwstr.as_ptr() as *const _));

        if display_name.is_empty() {
            anyhow::bail!("Empty shell folder item name");
        }

        Ok(display_name)
    }
}

fn get_name_from_registry(aumid: &str) -> Result<String> {
    let key_path = format!("SOFTWARE\\Classes\\AppUserModelId\\{aumid}");
    let key = HKLM.open_subkey(&key_path)?;

    let name: String = key.get_value("DisplayName")?;

    let display_name = if name.starts_with('@') { resolve_indirect_string(&name)? } else { name };

    if display_name.is_empty() {
        anyhow::bail!("Empty registry display name");
    }
    Ok(display_name)
}

fn resolve_indirect_string(indirect: &str) -> Result<String> {
    let input = HSTRING::from(indirect);
    let mut buf = vec![0u16; 1024];

    unsafe {
        SHLoadIndirectString(PCWSTR(input.as_ptr()), buf.as_mut(), None)?;
    }

    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Ok(String::from_utf16_lossy(&buf[..end]))
}
