//! Length-prefixed frame codec, independent of any IO library.

use serde::{Serialize, de::DeserializeOwned};

/// Largest frame either side accepts.
pub const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    #[error("frame of {0} bytes exceeds the {MAX_FRAME_BYTES} byte limit")]
    TooLarge(usize),
    #[error("frame is empty")]
    Empty,
    #[error("frame is not valid UTF-8 JSON for this message: {0}")]
    Malformed(#[from] serde_json::Error),
}

/// Validate a length header before allocating a buffer for the body.
pub fn check_len(header: [u8; 4]) -> Result<usize, FrameError> {
    let len = u32::from_be_bytes(header) as usize;
    if len == 0 {
        return Err(FrameError::Empty);
    }
    if len > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge(len));
    }
    Ok(len)
}

pub fn encode<T: Serialize>(message: &T) -> Result<Vec<u8>, FrameError> {
    let body = serde_json::to_vec(message)?;
    if body.len() > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge(body.len()));
    }
    let mut out = Vec::with_capacity(body.len() + 4);
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

pub fn decode<T: DeserializeOwned>(body: &[u8]) -> Result<T, FrameError> {
    Ok(serde_json::from_slice(body)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_oversized_and_empty_headers() {
        assert!(matches!(check_len(((MAX_FRAME_BYTES + 1) as u32).to_be_bytes()), Err(FrameError::TooLarge(_))));
        assert!(matches!(check_len(0u32.to_be_bytes()), Err(FrameError::Empty)));
        assert_eq!(check_len(10u32.to_be_bytes()).unwrap(), 10);
    }

    #[test]
    fn encode_prefixes_length() {
        let bytes = encode(&serde_json::json!({"a": 1})).unwrap();
        let len = check_len(bytes[..4].try_into().unwrap()).unwrap();
        assert_eq!(len, bytes.len() - 4);
        let value: serde_json::Value = decode(&bytes[4..]).unwrap();
        assert_eq!(value["a"], 1);
    }

    #[test]
    fn malformed_body_is_an_error() {
        assert!(matches!(decode::<serde_json::Value>(b"{not json"), Err(FrameError::Malformed(_))));
    }
}
