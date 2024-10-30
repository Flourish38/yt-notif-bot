use std::fmt::Debug;

use crate::{HYPER, KEY, YOUTUBE};
use google_youtube3::{
    api::PlaylistItemContentDetails,
    chrono::{DateTime, Utc},
    hyper,
};
use hyper::{body, http::uri::InvalidUri, Body, Response, StatusCode};

#[derive(Debug)]
#[allow(dead_code)]
pub enum PlaylistIdError {
    UriParseError(InvalidUri),
    Hyper(hyper::Error),
    BadStatus(StatusCode),
    BodyParseError(String),
}

impl From<hyper::Error> for PlaylistIdError {
    fn from(value: hyper::Error) -> Self {
        Self::Hyper(value)
    }
}

impl From<InvalidUri> for PlaylistIdError {
    fn from(value: InvalidUri) -> Self {
        Self::UriParseError(value)
    }
}

pub async fn get_upload_playlist_id(
    channel_uri: impl Into<String>,
) -> Result<String, PlaylistIdError> {
    let mut channel_uri = channel_uri
        .into()
        .replace("youtu.be", "www.youtube.com")
        .replace("m.youtube.com", "www.youtube.com");
    // /search page is about 100KB smaller
    channel_uri.push_str("/search");

    let uri = channel_uri.clone().try_into()?;

    let response = HYPER.get().unwrap().get(uri).await?;

    let b = match response.status() {
        StatusCode::OK => Ok(response.into_body()),
        s => Err(PlaylistIdError::BadStatus(s)),
    }?;

    let bytes = body::to_bytes(b).await?;

    let prefix_bytes = *b"channel_id=";
    let mut prefix_index = 0;
    let mut buf = String::with_capacity(24);
    for byte in bytes {
        if prefix_index >= prefix_bytes.len() {
            if byte == b'"' {
                if buf.len() == 0 {
                    // just in case there's a starting quote, which there almost certainly isn't
                    continue;
                } else {
                    // ending quote, break the loop, we're done!
                    break;
                }
            } else {
                // channel Ids start "UC", and the corresponding upload playlist starts "UU"
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

    if buf.len() != 24 || &buf[0..2] != "UU" {
        Err(PlaylistIdError::BodyParseError(channel_uri))
    } else {
        Ok(buf)
    }
}

#[derive(Debug)]
pub enum MissingContent {
    ContentDetails,
    VideoId,
    VideoPublishedAt,
    VideoDuration,
}

#[derive(Debug)]
pub enum UploadsError {
    YouTube3(google_youtube3::Error),
    // Empty(PlaylistItemListResponse),
    MissingContent(MissingContent),
}

impl From<google_youtube3::Error> for UploadsError {
    fn from(value: google_youtube3::Error) -> Self {
        UploadsError::YouTube3(value)
    }
}

impl From<MissingContent> for UploadsError {
    fn from(value: MissingContent) -> Self {
        Self::MissingContent(value)
    }
}

#[derive(Debug, Clone)]
pub struct Video {
    pub id: String,
    pub published_at: DateTime<Utc>,
}

impl TryFrom<PlaylistItemContentDetails> for Video {
    type Error = MissingContent;

    fn try_from(value: PlaylistItemContentDetails) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.video_id.ok_or(MissingContent::VideoId)?,
            published_at: value
                .video_published_at
                .ok_or(MissingContent::VideoPublishedAt)?,
        })
    }
}

pub async fn get_uploads_from_playlist(playlist_id: &str) -> Result<Vec<Video>, UploadsError> {
    let response = YOUTUBE
        .get()
        .unwrap()
        .playlist_items()
        .list(&vec!["contentDetails".into()])
        .playlist_id(playlist_id)
        .max_results(50)
        .param("key", KEY.get().unwrap())
        .doit()
        .await?
        .1;

    match response.items {
        None => Ok(vec![]),
        Some(items) => Ok(items
            .into_iter()
            .map(|pi| {
                pi.content_details
                    .ok_or(MissingContent::ContentDetails)?
                    .try_into()
            })
            .collect::<Result<Vec<Video>, MissingContent>>()?),
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub enum DurationsError {
    YouTube3(google_youtube3::Error),
    MissingContent(MissingContent),
    Empty(Response<Body>),
    LengthMismatch(Vec<google_youtube3::api::Video>),
}

impl From<google_youtube3::Error> for DurationsError {
    fn from(value: google_youtube3::Error) -> Self {
        DurationsError::YouTube3(value)
    }
}

impl From<MissingContent> for DurationsError {
    fn from(value: MissingContent) -> Self {
        DurationsError::MissingContent(value)
    }
}

#[derive(Clone)]
pub struct VideoExtras {
    pub duration: String,
}

pub async fn get_videos_extras(videos: &[Video]) -> Result<Vec<VideoExtras>, DurationsError> {
    let mut query = YOUTUBE
        .get()
        .unwrap()
        .videos()
        .list(&vec!["contentDetails".into()]);
    for video in videos {
        query = query.add_id(video.id.as_str());
    }
    let response = query
        .max_results(50)
        .param("key", KEY.get().unwrap())
        .doit()
        .await?;

    let durations = match response.1.items {
        Some(v) => {
            if v.len() == videos.len() {
                v.into_iter()
                    .map(|v| {
                        Ok(VideoExtras {
                            duration: v
                                .content_details
                                .ok_or(MissingContent::ContentDetails)?
                                .duration
                                .ok_or(MissingContent::VideoDuration)?,
                        })
                    })
                    .collect::<Result<Vec<VideoExtras>, MissingContent>>()?
            } else {
                return Err(DurationsError::LengthMismatch(v));
            }
        }
        None => {
            if videos.len() == 0 {
                vec![]
            } else {
                return Err(DurationsError::Empty(response.0));
            }
        }
    };

    Ok(durations)
}
