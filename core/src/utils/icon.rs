use std::{ffi::OsStr, fs, os::windows::ffi::OsStrExt, path::{Path, PathBuf}};

use anyhow::{Result, anyhow};
use image::{ImageBuffer, Rgba};
use quick_xml::Reader;
use sysinfo::System;
use windows::Win32::{Graphics::Gdi::{BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDIBits, GetObjectW, HBITMAP, HDC}, UI::{Shell::{SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON, SHGetFileInfoW}, WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO}}};
use windows_core::PCWSTR;
use winreg::{RegKey, enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE}};

use crate::utils::cache_dir;

pub fn resolve_app_icon(app_id: &str) -> Result<Option<String>> {
    let cache_path = cache_path(app_id);

    if cache_path.exists() {
        return Ok(Some(cache_path.to_string_lossy().to_string()));
    }

    if app_id.contains("!") {
        resolve_packaged_icon(app_id, &cache_path)
    } else {
        resolve_win32_icon(app_id, &cache_path)
    }
}

fn resolve_packaged_icon(app_id: &str, cache_path: &Path) -> Result<Option<String>> {
    if cache_path.exists() {
        return Ok(Some(cache_path.to_string_lossy().to_string()));
    }

    let pfn = package_full_name_from_aumid(app_id)?;
    let install_root = install_path_from_pfn(&pfn)?;
    let logo_path = logo_path_from_manifest(&install_root)?;
    let icon_path = resolve_icon_file(&install_root, &logo_path)?;

    fs::create_dir_all(cache_path.parent().unwrap())?;

    fs::copy(&icon_path, &cache_path)?;

    Ok(Some(cache_path.to_string_lossy().to_string()))
}

fn package_full_name_from_aumid(app_id: &str) -> Result<String> {
    let subkey = format!(r"Software\Classes\AppUserModelId\{app_id}");

    for hive in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        let root = RegKey::predef(hive);

        if let Ok(key) = root.open_subkey(&subkey) {
            if let Ok(pfn) = key.get_value::<String, _>("PackageFullName") {
                return Ok(pfn);
            }
        }
    }

    Err(anyhow!("PackageFullName not found"))
}

fn install_path_from_pfn(pfn: &str) -> Result<PathBuf> {
    let path = format!(r"Software\Classes\Local Settings\Software\Microsoft\Windows\CurrentVersion\AppModel\Repository\Packages\{pfn}");

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = hkcu.open_subkey(path)?;
    let root: String = key.get_value("PackageRootFolder")?;

    Ok(PathBuf::from(root))
}

fn logo_path_from_manifest(install_root: &Path) -> Result<String> {
    let manifest = install_root.join("AppxManifest.xml");

    let xml = fs::read_to_string(manifest)?;

    let mut reader = Reader::from_str(&xml);
    reader.config_mut().trim_text(true);

    loop {
        match reader.read_event() {
            Ok(quick_xml::events::Event::Start(e)) | Ok(quick_xml::events::Event::Empty(e)) => {
                let local = e.local_name();
                let name = std::str::from_utf8(local.as_ref())
                    .unwrap_or("");

                if name == "VisualElements" {
                    for attr in e.attributes().flatten() {
                        let local_name = attr.key.local_name();
                        let key = std::str::from_utf8(local_name.as_ref())
                            .unwrap_or("");

                        if key == "Square44x44Logo" {
                            return Ok(String::from_utf8_lossy(&attr.value).to_string())
                        }
                    }
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(e) => return Err(e.into()),

            _ => {}
        }
    }

    Err(anyhow!("Square44x44Logo not found"))
}

fn resolve_icon_file(install_root: &Path, logo_path: &str) -> Result<PathBuf> {
    let normalized = logo_path.replace("\\", &String::from(std::path::MAIN_SEPARATOR));

    let exact = install_root.join(&normalized);

    if exact.exists() {
        return Ok(exact);
    }

    let base = install_root.join(&normalized);

    let stem = base.file_stem().unwrap().to_string_lossy();

    let dir = base.parent().unwrap();

    let pattern = format!("{}/{}", dir.display(), stem);

    let mut candidates = Vec::new();

    for path in glob::glob(&pattern)?.flatten() {
        candidates.push(path);
    }

    let preference = [
        "scale-200",
        "scale-150",
        "scale-100",
        "targetsize-32",
        "targetsize-24",
    ];

    for tag in preference {
        if let Some(path) = candidates.iter().find(|p| {
            p.file_name().and_then(|n| n.to_str()).map(|n| n.contains(tag)).unwrap_or(false)
        }) {
            return Ok(path.clone());
        }
    }

    candidates
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("No icon candidate found"))
}

fn resolve_win32_icon(app_id: &str, cache_path: &Path) -> Result<Option<String>> {
    let exe_path = find_process_exe(app_id)?;
    extract_icon_to_png(&exe_path, cache_path)?;

    Ok(Some(cache_path.to_string_lossy().to_string()))
}

fn cache_path(app_id: &str) -> PathBuf {
    let safe = app_id
        .replace("\\", "_")
        .replace("/", "_")
        .replace(":", "_")
        .replace("!", "_");

    cache_dir().join("icons").join(format!("{safe}.png"))
}

fn find_process_exe(app_id: &str) -> Result<PathBuf> {
    let mut sys = System::new_all();

    sys.refresh_all();

    for process in sys.processes().values() {
        let name = process.name().to_string_lossy().to_string();

        if name == app_id.to_ascii_lowercase() {
            if let Some(path) = process.exe() {
                return Ok(path.to_path_buf());
            }
        }
    }

    Err(anyhow!("Process not found: {app_id}"))
}

fn extract_icon_to_png(exe: &Path, output: &Path) -> Result<()> {
    fs::create_dir_all(output.parent().unwrap())?;

    let wide: Vec<u16> = OsStr::new(exe)
        .encode_wide()
        .chain(Some(0))
        .collect();

    unsafe {
        let mut info = SHFILEINFOW::default();

        SHGetFileInfoW(
            PCWSTR(wide.as_ptr()), 
            windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES(0), 
            Some(&mut info), 
            std::mem::size_of::<SHFILEINFOW>() as u32, 
            SHGFI_ICON | SHGFI_LARGEICON
        );

        if info.hIcon.is_invalid() {
            return Err(anyhow!("No icon"));
        }

        save_hicon_png(info.hIcon, output)?;

        DestroyIcon(info.hIcon)?;
    }

    Ok(())
}

fn save_hicon_png(icon: HICON, output: &Path) -> Result<()> {
    unsafe {
        let mut icon_info = ICONINFO::default();

        GetIconInfo(icon, &mut icon_info)?;

        let mut bitmap = BITMAP::default();

        GetObjectW(
            icon_info.hbmColor.into(), 
            std::mem::size_of::<BITMAP>() as i32, 
            Some(&mut bitmap as *mut _ as *mut _)
        );

        let width = bitmap.bmWidth as u32;
        let height = bitmap.bmHeight as u32;

        let mut pixels = vec![0u8; (width * height * 4) as usize];

        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: 0,
                ..Default::default()
            },
            ..Default::default()
        };

        let hdc = HDC::default();
        
        GetDIBits(
            hdc, 
            HBITMAP(icon_info.hbmColor.0), 
            0, 
            height, 
            Some(pixels.as_mut_ptr() as *mut _), 
            &mut bmi, 
            DIB_RGB_COLORS
        );

        let image = ImageBuffer::<Rgba<u8>, _>::from_raw(width, height, pixels)
            .ok_or_else(|| anyhow!("Invalid image"))?;

        image.save(output)?;

        let _ = DeleteObject(icon_info.hbmColor.into());
        let _ = DeleteObject(icon_info.hbmMask.into());
    }

    Ok(())
}