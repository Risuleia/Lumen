use anyhow::Result;
use windows::{ApplicationModel::Package, Media::Control::
    GlobalSystemMediaTransportControlsSession
, Storage::Streams::{DataReader, RandomAccessStreamReference}};

pub async fn get_thumbnail_bytes(
    session: &GlobalSystemMediaTransportControlsSession
) -> Result<Option<Vec<u8>>> {
    let props = session.TryGetMediaPropertiesAsync()?.await?;

    let thumbnail = props.Thumbnail().ok();

    let Some(thumbnail) = thumbnail else {
        return Ok(None);
    };

    let stream = thumbnail.OpenReadAsync()?.await?;
    let size = stream.Size()? as usize;

    if size == 0 {
        return Ok(None);
    }

    let input = stream.GetInputStreamAt(0)?;
    let reader = DataReader::CreateDataReader(&input)?;

    reader.LoadAsync(size as u32)?.await?;
    let mut buf = vec![0u8; size];
    reader.ReadBytes(&mut buf)?;

    Ok(Some(buf))
}

pub async fn get_app_icon_bytes(
    session: &GlobalSystemMediaTransportControlsSession
) -> Result<Option<Vec<u8>>> {
    let aumid = match session.SourceAppUserModelId().ok() {
        Some(id) => id.to_string_lossy(),
        None => return Ok(None)
    };

    let packages = Package::Current()?.Dependencies()?;

    for pkg in packages {
        if let Ok(id) = pkg.Id() {
            if let Ok(full) = id.FullName() {
                if !full.to_string_lossy().contains(&aumid) {
                    continue;
                }
            }

            if let Ok(logo_uri) = pkg.Logo() {
                let reference =
                    RandomAccessStreamReference::CreateFromUri(&logo_uri)?;

                let stream = reference.OpenReadAsync()?.await?;
                let size = stream.Size()? as u32;

                if size == 0 {
                    return Ok(None);
                }

                let input = stream.GetInputStreamAt(0)?;
                let reader = DataReader::CreateDataReader(&input)?;

                reader.InputStreamOptions()?;
                reader.LoadAsync(size)?.await?;

                let mut buf = vec![0u8; size as usize];
                reader.ReadBytes(&mut buf)?;

                return Ok(Some(buf));
            }
        }
    }

    Ok(None)
}