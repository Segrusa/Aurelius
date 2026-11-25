//! `upload.get`: the files the user uploaded with a message, task, recurring
//! task or idea, for clients that are not on this computer. The id names the file's
//! owner and its place in the owner's list (`aurelius_domain::conversation::upload_id`);
//! the path comes from that stored list, never from the client, and must lie
//! inside the work folder's uploads area (symlinks and junctions resolved).

use crate::Shared;
use aurelius_domain::conversation::{UPLOAD_MAX_BYTES, UploadOwner, parse_upload_id};
use aurelius_domain::views::MessageFileView;
use aurelius_protocol::{ErrorCode, ProtocolError, UPLOAD_CHUNK_BYTES, UPLOAD_THUMB_MAX_SIDE, UploadChunk};
use base64::Engine;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

/// Where uploads are kept, relative to the work folder (see `message_uploads`).
const UPLOADS_DIR: &str = ".aurelius/uploads";
/// Smallest thumbnail side asked for.
const THUMB_MIN_SIDE: u32 = 16;
/// Largest image side decoded for a thumbnail.
const MAX_DECODE_SIDE: u32 = 16_384;
const THUMB_QUALITY: u8 = 80;

pub async fn get(shared: &Arc<Shared>, id: String, offset: Option<u64>, max_side: Option<u32>) -> Result<UploadChunk, ProtocolError> {
    let Some((owner, index)) = parse_upload_id(&id) else { return Err(gone()) };
    let store = shared.store().clone();
    let worktrees = aurelius_domain::work_place::worktrees_dir(shared.store().root());
    let found = spawn(move || {
        store
            .read(|c| {
                use aurelius_storage::{conversations, ideas, task_schedules, tasks, tickets};
                Ok(match owner {
                    UploadOwner::Task(task) => tasks::attachments_of(c, task)?.map(|a| (vec![a.folder], a.files)),
                    UploadOwner::Schedule(schedule) => task_schedules::load(c, schedule)?.and_then(|r| r.attachments).map(|a| (vec![a.folder], a.files)),
                    UploadOwner::Idea(idea) => ideas::load(c, idea)?.and_then(|r| r.attachments).map(|a| (vec![a.folder], a.files)),
                    UploadOwner::Ticket(ticket) => tickets::attachments_of(c, ticket)?.map(|a| (vec![a.folder], a.files)),
                    UploadOwner::Message(message) => match conversations::message_uploads(c, message)? {
                        Some((conversation, files)) => Some((conversations::work_folders(c, &worktrees, conversation)?, files)),
                        None => None,
                    },
                })
            })
            .map_err(|e: aurelius_storage::StorageError| ProtocolError::new(ErrorCode::StorageUnavailable, e.to_string()))
    })
    .await?;
    let Some((folders, mut files)) = found else { return Err(gone()) };
    if index >= files.len() {
        return Err(gone());
    }
    let file = files.swap_remove(index);
    let roots: Vec<PathBuf> = folders.into_iter().map(PathBuf::from).collect();
    spawn(move || {
        let real = locate(&roots, &file)?;
        match max_side {
            Some(side) => thumbnail(&real, id, &file, side),
            None => chunk(&real, id, &file, offset.unwrap_or(0)),
        }
    })
    .await
}

async fn spawn<T: Send + 'static>(f: impl FnOnce() -> Result<T, ProtocolError> + Send + 'static) -> Result<T, ProtocolError> {
    tokio::task::spawn_blocking(f).await.map_err(|e| ProtocolError::new(ErrorCode::Internal, e.to_string()))?
}

fn gone() -> ProtocolError {
    ProtocolError::new(ErrorCode::NotFound, "This file is no longer kept.")
}

/// The real file of `file` in the first root that has it, inside that root's uploads area.
fn locate(roots: &[PathBuf], file: &MessageFileView) -> Result<PathBuf, ProtocolError> {
    let rel = Path::new(&file.path);
    let confined = file.path.starts_with(&format!("{UPLOADS_DIR}/")) && rel.components().all(|c| matches!(c, Component::Normal(_) | Component::CurDir));
    if !confined {
        return Err(ProtocolError::new(ErrorCode::Forbidden, "This file is outside the uploads area."));
    }
    for root in roots {
        // canonicalize resolves symlinks and junctions: what is left is the real place.
        let (Ok(area), Ok(real)) = (std::fs::canonicalize(root.join(UPLOADS_DIR)), std::fs::canonicalize(root.join(rel))) else { continue };
        if !real.starts_with(&area) {
            return Err(ProtocolError::new(ErrorCode::Forbidden, "This file is outside the uploads area."));
        }
        if real.is_file() {
            return Ok(real);
        }
    }
    Err(gone())
}

fn unreadable(e: std::io::Error) -> ProtocolError {
    ProtocolError::new(ErrorCode::Validation, format!("This file can't be read: {e}"))
}

fn chunk(real: &Path, id: String, file: &MessageFileView, offset: u64) -> Result<UploadChunk, ProtocolError> {
    use std::io::{Read, Seek, SeekFrom};
    let mut handle = std::fs::File::open(real).map_err(unreadable)?;
    let length = handle.metadata().map_err(unreadable)?.len();
    if length > UPLOAD_MAX_BYTES {
        return Err(ProtocolError::new(ErrorCode::Validation, format!("This file is too large to fetch (more than {} MB).", UPLOAD_MAX_BYTES / (1024 * 1024))));
    }
    if offset > length {
        return Err(ProtocolError::new(ErrorCode::Validation, format!("The offset {offset} is past the end of the file ({length} bytes).")));
    }
    handle.seek(SeekFrom::Start(offset)).map_err(unreadable)?;
    let mut bytes = Vec::new();
    handle.take(UPLOAD_CHUNK_BYTES.min(length - offset)).read_to_end(&mut bytes).map_err(unreadable)?;
    Ok(UploadChunk {
        id,
        name: file.name.clone(),
        media_type: aurelius_application::conversation::message_files::media_type(real).into(),
        byte_length: length,
        offset,
        done: offset + bytes.len() as u64 >= length,
        data_base64: base64::engine::general_purpose::STANDARD.encode(&bytes),
        thumbnail: false,
    })
}

/// A JPEG that fits `side` pixels (never larger than the image), transparency on white.
fn thumbnail(real: &Path, id: String, file: &MessageFileView, side: u32) -> Result<UploadChunk, ProtocolError> {
    let side = side.clamp(THUMB_MIN_SIDE, UPLOAD_THUMB_MAX_SIDE);
    let not_image = || ProtocolError::new(ErrorCode::Validation, "This file is not an image (PNG, JPEG, GIF, WebP or BMP), so it has no thumbnail.");
    let mut reader = image::ImageReader::open(real).map_err(unreadable)?.with_guessed_format().map_err(unreadable)?;
    if reader.format().is_none() {
        return Err(not_image());
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DECODE_SIDE);
    limits.max_image_height = Some(MAX_DECODE_SIDE);
    reader.limits(limits);
    let picture = reader.decode().map_err(|_| not_image())?;
    let picture = if picture.width().max(picture.height()) > side { picture.resize(side, side, image::imageops::FilterType::Triangle) } else { picture };
    let mut rgb = image::RgbImage::new(picture.width(), picture.height());
    for (to, from) in rgb.pixels_mut().zip(picture.to_rgba8().pixels()) {
        let alpha = from.0[3] as u32;
        *to = image::Rgb(std::array::from_fn(|i| ((from.0[i] as u32 * alpha + 255 * (255 - alpha)) / 255) as u8));
    }
    let mut out = Vec::new();
    rgb.write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, THUMB_QUALITY))
        .map_err(|e| ProtocolError::new(ErrorCode::Internal, e.to_string()))?;
    Ok(UploadChunk {
        id,
        name: file.name.clone(),
        media_type: "image/jpeg".into(),
        byte_length: out.len() as u64,
        offset: 0,
        done: true,
        data_base64: base64::engine::general_purpose::STANDARD.encode(&out),
        thumbnail: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn upload(dir: &tempfile::TempDir, name: &str, body: &[u8]) -> MessageFileView {
        let p = dir.path().join(UPLOADS_DIR).join(name);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, body).unwrap();
        MessageFileView { id: None, name: name.into(), path: format!("{UPLOADS_DIR}/{name}"), byte_length: body.len() as u64 }
    }

    fn roots(dir: &tempfile::TempDir) -> Vec<PathBuf> {
        vec![dir.path().to_path_buf()]
    }

    #[test]
    fn reads_in_chunks() {
        let d = tempfile::tempdir().unwrap();
        let body: Vec<u8> = (0..UPLOAD_CHUNK_BYTES + 10).map(|i| (i % 251) as u8).collect();
        let file = upload(&d, "big.bin", &body);
        let real = locate(&roots(&d), &file).unwrap();
        let first = chunk(&real, "x".into(), &file, 0).unwrap();
        assert_eq!((first.byte_length, first.done, first.thumbnail), (body.len() as u64, false, false));
        let second = chunk(&real, "x".into(), &file, UPLOAD_CHUNK_BYTES).unwrap();
        assert!(second.done);
        let decode = |c: &UploadChunk| base64::engine::general_purpose::STANDARD.decode(&c.data_base64).unwrap();
        assert_eq!([decode(&first), decode(&second)].concat(), body);
        assert_eq!(first.media_type, "application/octet-stream");
        assert_eq!(chunk(&real, "x".into(), &file, body.len() as u64 + 1).unwrap_err().code, ErrorCode::Validation);
        let empty = upload(&d, "empty.txt", b"");
        let whole = chunk(&locate(&roots(&d), &empty).unwrap(), "x".into(), &empty, 0).unwrap();
        assert!(whole.done && whole.data_base64.is_empty());
    }

    #[test]
    fn refuses_paths_outside_the_uploads_area() {
        let d = tempfile::tempdir().unwrap();
        upload(&d, "ok.txt", b"ok");
        std::fs::write(d.path().join("secret.txt"), b"s").unwrap();
        let at = |path: &str| MessageFileView { id: None, name: "x".into(), path: path.into(), byte_length: 1 };
        for path in ["secret.txt", ".aurelius/uploads/../../secret.txt", "/etc/passwd", ".aurelius/uploadsx/a", "C:/Windows/win.ini"] {
            assert_eq!(locate(&roots(&d), &at(path)).unwrap_err().code, ErrorCode::Forbidden, "{path}");
        }
        assert_eq!(locate(&roots(&d), &at(".aurelius/uploads/gone.txt")).unwrap_err().code, ErrorCode::NotFound);
        assert_eq!(locate(&[], &at(".aurelius/uploads/ok.txt")).unwrap_err().code, ErrorCode::NotFound);
        // The second work folder holds it when the first does not.
        let other = tempfile::tempdir().unwrap();
        assert!(locate(&[other.path().to_path_buf(), d.path().to_path_buf()], &at(".aurelius/uploads/ok.txt")).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn refuses_a_symlink_that_leaves_the_uploads_area() {
        let d = tempfile::tempdir().unwrap();
        upload(&d, "ok.txt", b"ok");
        std::fs::write(d.path().join("secret.txt"), b"s").unwrap();
        std::os::unix::fs::symlink(d.path().join("secret.txt"), d.path().join(UPLOADS_DIR).join("link.txt")).unwrap();
        let file = MessageFileView { id: None, name: "link.txt".into(), path: format!("{UPLOADS_DIR}/link.txt"), byte_length: 1 };
        assert_eq!(locate(&roots(&d), &file).unwrap_err().code, ErrorCode::Forbidden);
    }

    #[test]
    fn thumbnails_fit_and_only_images_have_one() {
        let d = tempfile::tempdir().unwrap();
        let mut png = Vec::new();
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(800, 400, image::Rgba([200, 30, 30, 0])))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let file = upload(&d, "shot.png", &png);
        let real = locate(&roots(&d), &file).unwrap();
        let thumb = thumbnail(&real, "x".into(), &file, 200).unwrap();
        assert!(thumb.thumbnail && thumb.done && thumb.media_type == "image/jpeg");
        let back = image::load_from_memory(&base64::engine::general_purpose::STANDARD.decode(&thumb.data_base64).unwrap()).unwrap();
        assert_eq!((back.width(), back.height()), (200, 100));
        // Fully transparent becomes white, not black.
        assert!(back.to_rgb8().get_pixel(10, 10).0.iter().all(|&v| v > 240));
        // Never upscaled; huge asks are clamped.
        let small = thumbnail(&real, "x".into(), &file, 5000).unwrap();
        let back = image::load_from_memory(&base64::engine::general_purpose::STANDARD.decode(&small.data_base64).unwrap()).unwrap();
        assert_eq!(back.width(), 800);
        let text = upload(&d, "notes.txt", b"hello");
        let err = thumbnail(&locate(&roots(&d), &text).unwrap(), "x".into(), &text, 200).unwrap_err();
        assert!(err.message.contains("not an image"), "{}", err.message);
    }
}
