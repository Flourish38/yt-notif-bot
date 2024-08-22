use std::fmt::Debug;

use crate::HYPER;
use google_youtube3::hyper;
use hyper::{body, http::uri::InvalidUri, StatusCode};

#[derive(Debug)]
#[allow(dead_code)]
pub enum ChannelIdError {
    UriParseError(InvalidUri),
    Hyper(hyper::Error),
    BadStatus(StatusCode),
    BodyParseError(String),
}

impl From<hyper::Error> for ChannelIdError {
    fn from(value: hyper::Error) -> Self {
        Self::Hyper(value)
    }
}

impl From<InvalidUri> for ChannelIdError {
    fn from(value: InvalidUri) -> Self {
        Self::UriParseError(value)
    }
}

pub async fn get_upload_playlist_id(
    channel_uri: impl Into<String>,
) -> Result<String, ChannelIdError> {
    let channel_uri = channel_uri.into();
    let mut channel_uri_buf = channel_uri.clone();
    channel_uri_buf.push_str("/search");

    let uri = channel_uri_buf.try_into()?;

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

    if buf.len() != 24 || &buf[0..2] == "UU" {
        Err(ChannelIdError::BodyParseError(channel_uri))
    } else {
        Ok(buf)
    }
}
