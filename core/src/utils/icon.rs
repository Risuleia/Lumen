use std::{fs, path::{Path, PathBuf}};

use anyhow::{Result, anyhow};
use image::{DynamicImage, GenericImageView};
use windows::{ApplicationModel::AppInfo, Foundation::Size, Storage::Streams::{Buffer, DataReader, InputStreamOptions}, Win32::{Foundation::SIZE, Graphics::Gdi::{BI_RGB, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC, GetDIBits, HBITMAP, ReleaseDC}, UI::Shell::{IShellItem2, IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_RESIZETOFIT}}};
use windows_core::{HSTRING, Interface, PCWSTR};
use winreg::HKLM;

use crate::utils::cache_dir;

struct OwnedDC(windows::Win32::Graphics::Gdi::HDC);

impl Drop for OwnedDC {
    fn drop(&mut self) {
        unsafe { ReleaseDC(None, self.0); }
    }
}

pub async fn resolve_app_icon(aumid: &str) -> Option<String> {
    let cache_path = cache_path(aumid);

    if cache_path.exists() {
        return Some(cache_path.to_string_lossy().to_string());
    }

    if let Ok(path) = get_logo(aumid, &cache_path).await { return path; }
    if let Ok(path) = get_win32_icon(aumid, &cache_path) { return path;}
    if let Ok(path) = get_icon_from_registry(aumid, &cache_path) { return path;}

    None
}


async fn get_logo(aumid: &str, cache_path: &Path) -> Result<Option<String>> {
    if cache_path.exists() {
        return Ok(Some(cache_path.to_string_lossy().to_string()));
    }

    let aumid_hstring = HSTRING::from(aumid);

    let app_info = AppInfo::GetFromAppUserModelId(&aumid_hstring)?;
    let display_info = app_info.DisplayInfo()?;

    let logo_stream_reference = display_info
        .GetLogo(Size { Width: 64.0, Height: 64.0 })?;

    let stream = logo_stream_reference.OpenReadAsync()?.await?;
    let size = stream.Size()? as u32;

    let buffer = Buffer::Create(size)?;

    stream
        .ReadAsync(&buffer, size, InputStreamOptions::None)?
        .await?;

    let reader = DataReader::FromBuffer(&buffer)?;

    let mut bytes = vec![0u8; size as usize];
    reader.ReadBytes(&mut bytes)?;

    let img = image::load_from_memory(&bytes)?;
    let img = process_logo(img);

    fs::create_dir_all(cache_path.parent().unwrap())?;

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
        let shell_item: IShellItem2 = SHCreateItemFromParsingName(
            PCWSTR(path_hstring.as_ptr()), 
            None
        )?;

        let image_factory: IShellItemImageFactory = shell_item.cast()?;

        let size = SIZE { cx: 64, cy: 64 };
        let hbitmap: HBITMAP = image_factory.GetImage(size, SIIGBF_RESIZETOFIT)?;

        std::fs::create_dir_all(cache_path.parent().unwrap())?;

        let save_res = save_hbitmap_to_png(hbitmap, cache_path);

        let _ = DeleteObject(hbitmap.into());

        save_res?;
    }

    Ok(Some(cache_path.to_string_lossy().to_string()))
}

fn get_icon_from_registry(aumid: &str, cache_path: &Path) -> Result<Option<String>> {
    let key_path = format!("SOFTWARE\\Classes\\AppUserModelId\\{aumid}");
    let key = HKLM.open_subkey(&key_path)?;

    let display_path: String = key.get_value("IconUri")?;
    let path = Path::new(&display_path);
    if !path.exists() {
        return Err(anyhow!("Path in IconUri doesn't exist"));
    }

    let img = image::open(path)?;
    let img = process_logo(img);

    std::fs::create_dir_all(cache_path.parent().unwrap())?;
    
    img.save_with_format(cache_path, image::ImageFormat::Png)?;

    Ok(Some(cache_path.to_string_lossy().to_string()))
}

unsafe fn save_hbitmap_to_png(hbitmap: HBITMAP, cache_path: &Path) -> Result<()> {
    unsafe {
        let hdc = OwnedDC(GetDC(None));

        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: 64,
                biHeight: -64,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0 as u32,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut buf = vec![0u8; 64 * 64 * 4];

        GetDIBits(
            hdc.0, 
            hbitmap, 
            0, 
            64, 
            Some(buf.as_mut_ptr() as *mut _), 
            &mut bmi, 
            DIB_RGB_COLORS
        );

        let mut has_alpha = false;

        for chunk in buf.chunks_exact_mut(4) {
            chunk.swap(0, 2);

            if chunk[3] != 0 {
                has_alpha = true;
            }
        }

        if !has_alpha {
            for chunk in buf.chunks_exact_mut(4) {
                chunk[3] = 255;
            }
        }

        image::save_buffer(
            cache_path, 
            &buf, 
            64, 
            64, 
            image::ColorType::Rgba8
        )?;

        Ok(())
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
                if x < min_x { min_x = x; }
                if x > max_x { max_x = x; }
                if y < min_y { min_y = y; }
                if y > max_y { max_y = y; }
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

fn cache_path(aumid: &str) -> PathBuf {
    let safe = aumid.replace(
        ['\\', '/', ':', '*', '?', '"', '<', '>', '|'], 
        "_"
    );

    cache_dir().join("icons").join(format!("{safe}.png"))
}