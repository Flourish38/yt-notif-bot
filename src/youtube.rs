use std::fmt::Debug;

use crate::HYPER;
use google_youtube3::hyper;
use hyper::{body, StatusCode, Uri};

#[derive(Debug)]
pub enum ChannelIdError {
    UriParseError(<String as TryInto<Uri>>::Error),
    Hyper(hyper::Error),
    BadStatus(StatusCode),
}

impl From<hyper::Error> for ChannelIdError {
    fn from(value: hyper::Error) -> Self {
        Self::Hyper(value)
    }
}

pub async fn get_upload_playlist_id(
    channel_uri: impl Into<String>,
) -> Result<String, ChannelIdError> {
    let mut channel_uri = channel_uri.into();
    channel_uri.push_str("/search");

    let uri = channel_uri
        .try_into()
        .map_err(|e| ChannelIdError::UriParseError(e))?;

    let response = HYPER.get().unwrap().get(uri).await?;

    let b = match response.status() {
        StatusCode::OK => Ok(response.into_body()),
        s => Err(ChannelIdError::BadStatus(s)),
    }?;

    let bytes = body::to_bytes(b).await?;

    let prefix_bytes = *b"channel_id=";
    let mut prefix_index = 0;
    let mut buf = String::with_capacity(24);
    for byte in bytes {
        if prefix_index >= prefix_bytes.len() {
            if byte == b'"' {
                if buf.len() == 0 {
                    continue;
                } else {
                    break;
                }
            } else {
                if buf.len() == 1 && byte == b'C' {
                    buf.push('U');
                } else {
                    buf.push(byte as char);
                }
            }
        } else if byte == prefix_bytes[prefix_index] {
            prefix_index += 1;
        } else {
            prefix_index = 0;
        }
    }

    assert!(buf.len() == 24);
    assert!(&buf[0..2] == "UU");

    Ok(buf)
}
