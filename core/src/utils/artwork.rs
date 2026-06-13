use anyhow::Result;
use windows::{Media::Control::GlobalSystemMediaTransportControlsSessionMediaProperties, Storage::Streams::{Buffer, DataReader, InputStreamOptions}};
use xxhash_rust::xxh3::xxh3_64;

use crate::utils::cache_dir;

pub async fn extract_album_art(
    props: &GlobalSystemMediaTransportControlsSessionMediaProperties
) -> Result<Option<String>> {
    let thumbnail = match props.Thumbnail() {
        Ok(t) => t,
        Err(_) => return Ok(None)
    };

    let stream = thumbnail.OpenReadAsync()?.await?;

    let size = stream.Size()? as u32;

    let buffer = Buffer::Create(size)?;

    stream
        .ReadAsync(&buffer, size, InputStreamOptions::None)?
        .await?;

    let reader = DataReader::FromBuffer(&buffer)?;

    let mut bytes = vec![0u8; size as usize];
    reader.ReadBytes(&mut bytes)?;
    
    let hash = format!("{:016x}", xxh3_64(&bytes));

    let img = image::load_from_memory(&bytes)?;

    let dir = cache_dir().join("artwork");

    std::fs::create_dir_all(&dir)?;

    let path = dir.join(format!("{hash}.png"));

    if !path.exists() {
        img.save(&path)?;
    }

    Ok(Some(path.to_string_lossy().to_string()))
}