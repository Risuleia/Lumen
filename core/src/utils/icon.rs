use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Result, anyhow};
use image::{DynamicImage, GenericImageView};
use windows::{
    ApplicationModel::AppInfo,
    Foundation::Size,
    Storage::Streams::{Buffer, DataReader, InputStreamOptions},
    Win32::{
        Foundation::SIZE,
        Graphics::Gdi::{
            BI_RGB, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC, GetDIBits,
            HBITMAP, ReleaseDC,
        },
        UI::Shell::{
            IShellItem2, IShellItemImageFactory, SHCreateItemFromParsingName, SHLoadIndirectString,
            SIIGBF_ICONONLY, SIIGBF_RESIZETOFIT,
        },
    },
};
use windows_core::{HSTRING, Interface, PCWSTR};
use winreg::HKLM;

use crate::utils::icons_dir;

const LOGO_SIZE: f32 = 128.0;

struct OwnedDC(windows::Win32::Graphics::Gdi::HDC);

impl Drop for OwnedDC {
    fn drop(&mut self) {
        unsafe {
            ReleaseDC(None, self.0);
        }
    }
}

pub async fn resolve_app_icon(aumid: &str) -> Option<String> {
    let cache_path = cache_path(aumid);

    if cache_path.exists() {
        return Some(cache_path.to_string_lossy().to_string());
    }

    if aumid.to_lowercase().ends_with(".exe") {
        if let Ok(Some(path)) = get_exe_icon(aumid, &cache_path) {
            return Some(path);
        }
    }

    if let Ok(path) = get_logo(aumid, &cache_path).await {
        return path;
    }
    if let Ok(path) = get_win32_icon(aumid, &cache_path) {
        return path;
    }
    if let Ok(path) = get_icon_from_registry(aumid, &cache_path) {
        return path;
    }

    None
}

async fn get_logo(aumid: &str, cache_path: &Path) -> Result<Option<String>> {
    if cache_path.exists() {
        return Ok(Some(cache_path.to_string_lossy().to_string()));
    }

    let aumid_hstring = HSTRING::from(aumid);

    let app_info = AppInfo::GetFromAppUserModelId(&aumid_hstring)?;
    let display_info = app_info.DisplayInfo()?;

    let logo_stream_reference =
        display_info.GetLogo(Size { Width: LOGO_SIZE, Height: LOGO_SIZE })?;

    let stream = logo_stream_reference.OpenReadAsync()?.await?;
    let size = stream.Size()? as u32;
    if size == 0 {
        return Err(anyhow!("Empty stream logo"));
    }

    let buffer = Buffer::Create(size)?;

    stream.ReadAsync(&buffer, size, InputStreamOptions::None)?.await?;

    let reader = DataReader::FromBuffer(&buffer)?;

    let mut bytes = vec![0u8; size as usize];
    reader.ReadBytes(&mut bytes)?;

    let img = image::load_from_memory(&bytes)?;
    let img = process_logo(img);

    fs::create_dir_all(icons_dir())?;

    if !cache_path.exists() {
        img.save(&cache_path)?;
    }

    Ok(Some(cache_path.to_string_lossy().to_string()))
}

fn get_win32_icon(aumid: &str, cache_path: &Path) -> Result<Option<String>> {
    if cache_path.exists() {
        return Ok(Some(cache_path.to_string_lossy().to_string()));
    }

    let path = format!("shell:AppsFolder\\{aumid}");
    let path_hstring = HSTRING::from(&path);

    unsafe {
        let shell_item: IShellItem2 =
            SHCreateItemFromParsingName(PCWSTR(path_hstring.as_ptr()), None)?;

        let image_factory: IShellItemImageFactory = shell_item.cast()?;

        let size = SIZE { cx: LOGO_SIZE as i32, cy: LOGO_SIZE as i32 };
        let flags = SIIGBF_RESIZETOFIT.0 | SIIGBF_ICONONLY.0;
        let hbitmap: HBITMAP = image_factory.GetImage(size, std::mem::transmute(flags))?;

        let img_res = convert_hbitmap_to_image(hbitmap);
        let _ = DeleteObject(hbitmap.into());
        let img = img_res?;

        let processed_img = process_logo(img);

        // 3. Save it as a uniform 64x64 PNG
        std::fs::create_dir_all(icons_dir())?;
        processed_img.save(cache_path)?;
    }

    Ok(Some(cache_path.to_string_lossy().to_string()))
}

fn get_icon_from_registry(aumid: &str, cache_path: &Path) -> Result<Option<String>> {
    let key_path = format!("SOFTWARE\\Classes\\AppUserModelId\\{aumid}");
    let key = HKLM.open_subkey(&key_path)?;

    let raw_display_path: String = key.get_value("IconUri")?;
    let display_path = resolve_indirect_string(&raw_display_path);
    let path_wstr = HSTRING::from(&display_path);

    let expanded_path = unsafe {
        let mut buf = vec![0u16; 1024];
        let len = windows::Win32::System::Environment::ExpandEnvironmentStringsW(
            PCWSTR(path_wstr.as_ptr()),
            Some(&mut buf),
        ) as usize;
        if len > 0 && len <= buf.len() {
            String::from_utf16_lossy(&buf[..len - 1])
        } else {
            display_path.clone()
        }
    };

    if expanded_path.contains("ms-resource") || expanded_path.contains("ms-appx") {
        return Err(anyhow!("Cannot natively parse indirect UWP registry URIs"));
    }

    let path = Path::new(&display_path);
    if !path.exists() {
        return Err(anyhow!("Path in IconUri doesn't exist"));
    }

    let img = image::open(path)?;
    let img = process_logo(img);

    std::fs::create_dir_all(icons_dir())?;

    img.save_with_format(cache_path, image::ImageFormat::Png)?;

    Ok(Some(cache_path.to_string_lossy().to_string()))
}

fn get_exe_icon(exe_name: &str, cache_path: &Path) -> Result<Option<String>> {
    let mut target_exe_path: Option<PathBuf> = None;

    if let Ok(appdata) = std::env::var("APPDATA") {
        let appdata_path = PathBuf::from(appdata).join("Spotify").join(exe_name);
        if appdata_path.exists() {
            target_exe_path = Some(appdata_path);
        }
    }

    if target_exe_path.is_none() {
        if let Ok(path_var) = std::env::var("PATH") {
            for dir in std::env::split_paths(&path_var) {
                let full_path = dir.join(exe_name);
                if full_path.exists() {
                    target_exe_path = Some(full_path);
                    break;
                }
            }
        }
    }

    let Some(exe_path) = target_exe_path else {
        return Err(anyhow!("Could not locate raw path for bare executable: {exe_name}"));
    };

    let path_hstring = HSTRING::from(exe_path.to_string_lossy().as_ref());

    unsafe {
        let shell_item: IShellItem2 =
            SHCreateItemFromParsingName(PCWSTR(path_hstring.as_ptr()), None)?;

        let image_factory: IShellItemImageFactory = shell_item.cast()?;
        let size = SIZE { cx: LOGO_SIZE as i32, cy: LOGO_SIZE as i32 };
        let flags = SIIGBF_RESIZETOFIT.0 | SIIGBF_ICONONLY.0;

        let hbitmap: HBITMAP = image_factory.GetImage(size, std::mem::transmute(flags))?;
        let img_res = convert_hbitmap_to_image(hbitmap);
        let _ = DeleteObject(hbitmap.into());
        let img = img_res?;

        let processed_img = process_logo(img);
        std::fs::create_dir_all(icons_dir())?;
        processed_img.save(cache_path)?;
    }

    Ok(Some(cache_path.to_string_lossy().to_string()))
}

unsafe fn convert_hbitmap_to_image(hbitmap: HBITMAP) -> Result<DynamicImage> {
    unsafe {
        let hdc = OwnedDC(GetDC(None));

        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: LOGO_SIZE as i32,
                biHeight: -(LOGO_SIZE as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0 as u32,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut buf = vec![0u8; (LOGO_SIZE as usize) * (LOGO_SIZE as usize) * 4];

        GetDIBits(
            hdc.0,
            hbitmap,
            0,
            64,
            Some(buf.as_mut_ptr() as *mut _),
            &mut bmi,
            DIB_RGB_COLORS,
        );

        for chunk in buf.chunks_exact_mut(4) {
            let b = chunk[0] as f32;
            let g = chunk[1] as f32;
            let r = chunk[2] as f32;
            let alpha = chunk[3] as f32 / 255.0;

            if alpha > 0.0 {
                chunk[0] = ((r / alpha).min(255.0)) as u8;
                chunk[1] = ((g / alpha).min(255.0)) as u8;
                chunk[2] = ((b / alpha).min(255.0)) as u8;
            } else {
                chunk[0] = 0;
                chunk[1] = 0;
                chunk[2] = 0;
            }
        }

        let img = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(
            LOGO_SIZE as u32,
            LOGO_SIZE as u32,
            buf,
        )
        .ok_or_else(|| anyhow!("Failed to construct image buffer from HBITMAP"))?;

        Ok(DynamicImage::ImageRgba8(img))
    }
}

fn process_logo(img: DynamicImage) -> DynamicImage {
    let (width, height) = img.dimensions();
    let mut min_x = width;
    let mut max_x = 0;
    let mut min_y = height;
    let mut max_y = 0;
    let mut has_pixels = false;

    for x in 0..width {
        for y in 0..height {
            let pixel = img.get_pixel(x, y);
            if pixel[3] > 0 {
                if x < min_x {
                    min_x = x;
                }
                if x > max_x {
                    max_x = x;
                }
                if y < min_y {
                    min_y = y;
                }
                if y > max_y {
                    max_y = y;
                }
                has_pixels = true;
            }
        }
    }

    if has_pixels {
        let crop_w = max_x - min_x + 1;
        let crop_h = max_y - min_y + 1;

        let cropped = img.crop_imm(min_x, min_y, crop_w, crop_h);
        cropped.resize(64, 64, image::imageops::FilterType::Lanczos3)
    } else {
        img.resize_exact(64, 64, image::imageops::FilterType::Lanczos3)
    }
}

fn resolve_indirect_string(source: &str) -> String {
    if !source.starts_with('@') {
        return source.to_string();
    }

    let source_w = HSTRING::from(source);
    let mut out_buf = vec![0u16; 1024];

    unsafe {
        let result = SHLoadIndirectString(PCWSTR(source_w.as_ptr()), out_buf.as_mut(), None);

        if result.is_ok() {
            if let Some(null_pos) = out_buf.iter().position(|&c| c == 0) {
                return String::from_utf16_lossy(&out_buf[..null_pos]);
            }
        }
    }

    source.to_string()
}

fn cache_path(aumid: &str) -> PathBuf {
    let safe = aumid.replace(['\\', '/', ':', '*', '?', '"', '<', '>', '|'], "_");

    icons_dir().join(format!("{safe}.png"))
}
