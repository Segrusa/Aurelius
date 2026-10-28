//! Text extraction for attachments: plain text, Markdown, CSV,
//! JSON and text PDFs. Scanned PDFs and binary formats get a clear message.
//! PDF parsing runs on a separate thread with a time limit and panic guard.

use std::time::Duration;

/// Largest file read; the same as `aurelius_domain::conversation::UPLOAD_MAX_BYTES`
/// (this crate does not depend on the domain; a test in the application checks it).
pub const MAX_FILE_BYTES: usize = 25 * 1024 * 1024;
pub const MAX_TEXT_CHARS: usize = 200_000;
const PDF_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, PartialEq)]
pub struct Extracted {
    pub text: String,
    pub kind: &'static str,
    pub truncated: bool,
    pub extractor: &'static str,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ExtractError {
    #[error("The file is too large (at most {} MB).", MAX_FILE_BYTES / 1024 / 1024)]
    TooLarge,
    #[error("This file type is not supported yet ({0}); supported: TXT, MD, CSV, JSON, text PDF.")]
    Unsupported(String),
    #[error("This PDF has no text layer (scanned?); OCR is not supported yet.")]
    NoText,
    #[error("The PDF could not be read: {0}")]
    Pdf(String),
}

fn truncate(text: String) -> (String, bool) {
    if text.chars().count() <= MAX_TEXT_CHARS {
        return (text, false);
    }
    (text.chars().take(MAX_TEXT_CHARS).collect(), true)
}

pub fn extract(name: &str, bytes: &[u8]) -> Result<Extracted, ExtractError> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(ExtractError::TooLarge);
    }
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default();
    if ext == "pdf" || bytes.starts_with(b"%PDF") {
        return extract_pdf(bytes);
    }
    let text = std::str::from_utf8(bytes)
        .map(str::to_owned)
        .or_else(|_| std::str::from_utf8(bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes)).map(str::to_owned))
        .map_err(|_| ExtractError::Unsupported(if ext.is_empty() { "binary".into() } else { ext.clone() }))?;
    if text.contains('\0') {
        return Err(ExtractError::Unsupported(ext));
    }
    let kind = match ext.as_str() {
        "md" | "markdown" => "markdown",
        "csv" | "tsv" => "csv",
        "json" => {
            if serde_json::from_str::<serde_json::Value>(&text).is_err() {
                "text"
            } else {
                "json"
            }
        }
        _ => "text",
    };
    let (text, truncated) = truncate(text);
    Ok(Extracted { text, kind, truncated, extractor: "utf8-v1" })
}

fn extract_pdf(bytes: &[u8]) -> Result<Extracted, ExtractError> {
    let data = bytes.to_vec();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("pdf-extract".into())
        .spawn(move || {
            let result = std::panic::catch_unwind(|| pdf_extract::extract_text_from_mem(&data).map_err(|e| e.to_string()));
            let _ = tx.send(result.unwrap_or_else(|_| Err("the PDF parser crashed on this file".into())));
        })
        .map_err(|e| ExtractError::Pdf(e.to_string()))?;
    let text = rx.recv_timeout(PDF_TIMEOUT).map_err(|_| ExtractError::Pdf("time-out".into()))?.map_err(ExtractError::Pdf)?;
    if text.trim().len() < 20 {
        return Err(ExtractError::NoText);
    }
    let (text, truncated) = truncate(text);
    Ok(Extracted { text, kind: "pdf", truncated, extractor: "pdf-extract-0.12" })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_formats_and_refusals() {
        assert_eq!(extract("a.md", b"# Titel").unwrap().kind, "markdown");
        assert_eq!(extract("a.csv", b"a,b\n1,2").unwrap().kind, "csv");
        assert_eq!(extract("a.json", br#"{"x":1}"#).unwrap().kind, "json");
        assert!(matches!(extract("a.png", &[0x89, b'P', b'N', b'G', 0, 1, 2]), Err(ExtractError::Unsupported(_))));
        assert_eq!(extract("big.txt", &vec![b'a'; MAX_FILE_BYTES + 1]), Err(ExtractError::TooLarge));
    }

    #[test]
    fn broken_pdf_is_an_error_not_a_crash() {
        assert!(extract("x.pdf", b"%PDF-1.4 garbage").is_err());
    }
}
