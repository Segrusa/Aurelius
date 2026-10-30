//! Images the user sends with a message (pasted, picked or photographed).
//! Each is checked to really be an image, scaled down to what a model reads
//! well and kept with the message; the original file is not needed after.

use crate::{domain_error, storage_error};
use aurelius_domain::{DomainError, MessageId, conversation::MessageRole};
use aurelius_domain::{AppError};
use aurelius_domain::views::{FileRef, MessageView};
use aurelius_runtime::ImagePart;
use aurelius_storage::{PreparedBlob, StorageError, Store, Tx, blobs, conversations as conv};
use std::collections::HashMap;
use std::io::Cursor;
use std::path::Path;

/// Largest image file accepted.
pub const MAX_BYTES: u64 = aurelius_domain::conversation::UPLOAD_MAX_BYTES;
/// Longest side after scaling down (larger images are scaled down by the model anyway).
pub const MAX_SIDE: u32 = 1568;
/// Largest picture decoded, per side; guards against decompression bombs.
const MAX_DECODE_SIDE: u32 = 20_000;
const JPEG_QUALITY: u8 = 85;
/// Most images a run shows the model: the most recent ones of the conversation.
pub const MAX_SHOWN: usize = 8;

/// An image ready to be kept with a message.
#[derive(Debug, Clone)]
pub struct PreparedImage {
    pub blob: PreparedBlob,
    pub file: FileRef,
}

fn image_error(msg: impl Into<String>) -> AppError {
    domain_error(DomainError::File(msg.into()))
}

/// Decode, scale down and re-encode: JPEG, or PNG when the image has
/// transparent parts. Returns the bytes and their media type.
pub fn normalize(bytes: &[u8]) -> Result<(Vec<u8>, &'static str), String> {
    let mut reader = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format().map_err(|e| e.to_string())?;
    if reader.format().is_none() {
        return Err("Not an image (PNG, JPEG, GIF, WebP or BMP).".into());
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DECODE_SIDE);
    limits.max_image_height = Some(MAX_DECODE_SIDE);
    reader.limits(limits);
    let picture = reader.decode().map_err(|e| format!("Image not readable: {e}"))?;
    let picture = if picture.width().max(picture.height()) > MAX_SIDE {
        picture.resize(MAX_SIDE, MAX_SIDE, image::imageops::FilterType::Triangle)
    } else {
        picture
    };
    // Screenshots are often RGBA without any transparency: those become JPEG too.
    let transparent = picture.color().has_alpha() && picture.to_rgba8().pixels().any(|p| p.0[3] < 255);
    let mut out = Vec::new();
    if transparent {
        picture.write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png).map_err(|e| e.to_string())?;
        Ok((out, "image/png"))
    } else {
        let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY);
        picture.to_rgb8().write_with_encoder(encoder).map_err(|e| e.to_string())?;
        Ok((out, "image/jpeg"))
    }
}

/// Read and normalize the images at `paths` and write them as blobs
/// (registered later, in the command's transaction).
pub fn prepare(store: &Store, paths: &[String]) -> Result<Vec<PreparedImage>, AppError> {
    let mut out = Vec::new();
    for raw in paths {
        let path = Path::new(raw.trim());
        let shown = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "image".into());
        let metadata = std::fs::metadata(path).map_err(|e| image_error(format!("{shown}: not readable: {e}")))?;
        if !metadata.is_file() {
            return Err(image_error(format!("{shown}: choose an image, not a folder.")));
        }
        if metadata.len() > MAX_BYTES {
            return Err(image_error(format!("{shown}: larger than {} MB.", MAX_BYTES / (1024 * 1024))));
        }
        let bytes = std::fs::read(path).map_err(|e| image_error(format!("{shown}: not readable: {e}")))?;
        let (data, media_type) = normalize(&bytes).map_err(|e| image_error(format!("{shown}: {e}")))?;
        let extension = if media_type == "image/png" { "png" } else { "jpg" };
        let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).filter(|s| !s.is_empty()).unwrap_or_else(|| "image".into());
        let blob = store.blobs().put(&data, media_type).map_err(storage_error)?;
        let file = FileRef { id: blob.id.to_string(), name: format!("{stem}.{extension}"), media_type: media_type.into(), size: data.len() as u64 };
        out.push(PreparedImage { blob, file });
    }
    Ok(out)
}

/// Keep prepared images with a (just inserted) message.
pub fn keep(tx: &Tx<'_>, message: MessageId, images: &[PreparedImage]) -> Result<(), StorageError> {
    for image in images {
        blobs::register(tx, &image.blob)?;
    }
    let files: Vec<FileRef> = images.iter().map(|i| i.file.clone()).collect();
    conv::set_message_files(tx, message, &files)
}

/// The images of the user's messages in `history` (oldest first), per message:
/// the markers its text starts with and the images the model sees. All are
/// numbered and named; only the `MAX_SHOWN` most recent are shown.
pub fn for_run(
    conn: &rusqlite::Connection,
    store: &Store,
    history: &[&MessageView],
) -> Result<HashMap<MessageId, (String, Vec<ImagePart>)>, StorageError> {
    let sent: Vec<(MessageId, Vec<&FileRef>)> = history
        .iter()
        // The user's messages, and group chat messages forwarded to an employee (updates with the images).
        .filter(|m| (m.role == MessageRole::User && m.update.is_none()) || m.update.as_deref() == Some(aurelius_domain::group_chat::GROUP_KIND))
        .map(|m| (m.id, m.files.iter().filter(|f| f.media_type.starts_with("image/")).collect::<Vec<_>>()))
        .filter(|(_, files)| !files.is_empty())
        .collect();
    let total: usize = sent.iter().map(|(_, files)| files.len()).sum();
    let first_shown = total.saturating_sub(MAX_SHOWN) + 1;
    let mut number = 0;
    let mut out = HashMap::new();
    for (message, files) in sent {
        let mut markers = Vec::new();
        let mut parts = Vec::new();
        for file in files {
            number += 1;
            let part = if number >= first_shown {
                match conv::message_file(conn, &file.id)? {
                    Some((_, media_type, key)) => store.blobs().read(&key).ok().map(|data| ImagePart { name: file.name.clone(), media_type, data }),
                    None => None,
                }
            } else {
                None
            };
            markers.push(marker(number, &file.name, part.is_some()));
            parts.extend(part);
        }
        out.insert(message, (markers.join("\n"), parts));
    }
    Ok(out)
}

/// What the text says for an image, so the model can tell them apart (and
/// knows when an older one is no longer shown to it).
pub fn marker(number: usize, name: &str, shown: bool) -> String {
    if shown { format!("[Image {number}: {name}]") } else { format!("[Image {number}: {name} (sent earlier, no longer shown)]") }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32, alpha: u8) -> Vec<u8> {
        let picture = image::RgbaImage::from_pixel(width, height, image::Rgba([200, 30, 30, alpha]));
        let mut out = Vec::new();
        image::DynamicImage::ImageRgba8(picture).write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
        out
    }

    #[test]
    fn a_large_opaque_screenshot_becomes_a_smaller_jpeg() {
        let (bytes, media) = normalize(&png(3000, 1000, 255)).unwrap();
        assert_eq!(media, "image/jpeg");
        let back = image::load_from_memory(&bytes).unwrap();
        assert_eq!((back.width(), back.height()), (MAX_SIDE, 523));
    }

    #[test]
    fn transparency_stays_png_and_small_images_keep_their_size() {
        let (bytes, media) = normalize(&png(40, 20, 100)).unwrap();
        assert_eq!(media, "image/png");
        let back = image::load_from_memory(&bytes).unwrap();
        assert_eq!((back.width(), back.height()), (40, 20));
    }

    #[test]
    fn something_that_is_not_an_image_is_refused() {
        assert!(normalize(b"just some text, not a picture").is_err());
        assert!(normalize(b"\x89PNG\r\n\x1a\n broken").is_err());
    }

    #[test]
    fn prepared_images_are_named_after_the_file_with_their_new_type() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("data")).unwrap();
        let shot = dir.path().join("Scherm 1.bmp");
        let picture = image::RgbImage::from_pixel(10, 10, image::Rgb([1, 2, 3]));
        image::DynamicImage::ImageRgb8(picture).save(&shot).unwrap();
        let prepared = prepare(&store, &[shot.to_string_lossy().into_owned()]).unwrap();
        assert_eq!(prepared[0].file.name, "Scherm 1.jpg");
        assert_eq!(prepared[0].file.media_type, "image/jpeg");
        assert!(prepare(&store, &[dir.path().join("missing.png").to_string_lossy().into_owned()]).is_err());
    }

    #[test]
    fn markers_number_the_images() {
        assert_eq!(marker(3, "a.jpg", true), "[Image 3: a.jpg]");
        assert_eq!(marker(1, "b.png", false), "[Image 1: b.png (sent earlier, no longer shown)]");
    }
}
