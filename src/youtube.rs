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

pub async fn get_channel_id(channel_uri: impl Into<String>) -> Result<String, ChannelIdError> {
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

    println!("{}", bytes.len());

    Ok("".to_string())
}
