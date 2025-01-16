use std::{collections::HashMap, fmt::Debug};

use crate::{HYPER, KEY, YOUTUBE};
use google_youtube3::{
    api::PlaylistItemContentDetails,
    chrono::{DateTime, Utc},
    hyper,
};
use hyper::{body, http::uri::InvalidUri, Body, Response, StatusCode};
use serenity::all::{FormattedTimestamp, FormattedTimestampStyle};

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
    Snippet,
    CategoryId,
    VideoCategories,
    VideoCategoryTitle,
}

#[derive(Debug)]
#[allow(dead_code)]
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
        .use_with(|yt| async move {
            yt.playlist_items()
                .list(&vec!["contentDetails".into()])
                .playlist_id(playlist_id)
                .max_results(50)
                .param("key", KEY.get().unwrap())
                .doit()
                .await
        })
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
pub enum ShortsError {
    Hyper(hyper::Error),
    UriParseError(InvalidUri),
    BadStatus(StatusCode),
}

impl From<InvalidUri> for ShortsError {
    fn from(value: InvalidUri) -> Self {
        Self::UriParseError(value)
    }
}

impl From<hyper::Error> for ShortsError {
    fn from(value: hyper::Error) -> Self {
        Self::Hyper(value)
    }
}

pub async fn is_short(id: &str) -> Result<bool, ShortsError> {
    let uri = format!("https://www.youtube.com/shorts/{}", id).try_into()?;

    let response = HYPER.get().unwrap().get(uri).await?;

    match response.status() {
        StatusCode::SEE_OTHER => Ok(false), // 303
        StatusCode::OK => Ok(true),         // 200
        s => Err(ShortsError::BadStatus(s)),
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub enum ExtrasError {
    YouTube3(google_youtube3::Error),
    MissingContent(MissingContent),
    Empty(Response<Body>),
    LengthMismatch(Vec<google_youtube3::api::Video>),
    ShortsError(ShortsError),
}

impl From<google_youtube3::Error> for ExtrasError {
    fn from(value: google_youtube3::Error) -> Self {
        ExtrasError::YouTube3(value)
    }
}

impl From<MissingContent> for ExtrasError {
    fn from(value: MissingContent) -> Self {
        ExtrasError::MissingContent(value)
    }
}

impl From<ShortsError> for ExtrasError {
    fn from(value: ShortsError) -> Self {
        Self::ShortsError(value)
    }
}

#[derive(Clone)]
pub enum LiveStreamDetails {
    Live,
    VOD,
    Uploaded,
    Upcoming,
    NONSENSE,
}

#[derive(Clone)]
pub struct VideoExtras {
    pub time_string: String,
    pub category_id: String,
    pub live_stream_details: LiveStreamDetails,
    pub is_short: bool,
    pub is_scheduled: bool,
}

pub async fn get_videos_extras(videos: &[Video]) -> Result<Vec<VideoExtras>, ExtrasError> {
    if videos.len() == 0 {
        return Ok(vec![]);
    }

    let response = YOUTUBE
        .get()
        .unwrap()
        .use_with(|yt| async move {
            let mut query = yt.videos().list(&vec![
                "contentDetails".into(),
                "snippet".into(),
                "liveStreamingDetails".into(),
            ]);
            for video in videos {
                query = query.add_id(video.id.as_str());
            }
            query
                .max_results(50)
                .param("key", KEY.get().unwrap())
                .doit()
                .await
        })
        .await?;

    let Some(v) = response.1.items else {
        return Err(ExtrasError::Empty(response.0));
    };

    if v.len() != videos.len() {
        return Err(ExtrasError::LengthMismatch(v));
    }

    // random exactly-what-I-was-looking-for function that happens to be re-exported, we take those
    // found it from https://stackoverflow.com/a/68344457
    serenity::futures::future::try_join_all(v.into_iter().map(|v| async {
        let snippet = v.snippet.ok_or(MissingContent::Snippet)?;
        let duration = match v
            .content_details
            .ok_or(MissingContent::ContentDetails)
            .map(|cd| cd.duration)
            .transpose()
        {
            Some(r) => r,
            None => Err(MissingContent::VideoDuration),
        }
        .map(|d| format!("`({})`", d));
        // nightmare
        let (live_stream_details, time_string, is_scheduled) =
            if let Some(lsd) = v.live_streaming_details {
                (
                    match (
                        lsd.scheduled_start_time,
                        lsd.actual_start_time,
                        lsd.actual_end_time,
                    ) {
                        (Some(_), None, None) => LiveStreamDetails::Upcoming,
                        (_, Some(_), None) => LiveStreamDetails::Live,
                        (_, Some(_), Some(_)) => LiveStreamDetails::VOD,
                        (_, None, Some(_)) | (None, None, None) => LiveStreamDetails::NONSENSE, // yes, this can happen. https://youtu.be/m6KNUV71sxE
                    },
                    match (
                        lsd.scheduled_start_time,
                        lsd.actual_start_time,
                        lsd.actual_end_time,
                    ) {
                        (_, _, Some(_)) => duration?,
                        // cool that it lets me combine these like this
                        (Some(dt), None, None) | (_, Some(dt), None) => FormattedTimestamp::new(
                            dt.into(),
                            Some(FormattedTimestampStyle::RelativeTime),
                        )
                        .to_string(),
                        (None, None, None) => String::default(),
                    },
                    lsd.scheduled_start_time.is_some(),
                )
            } else {
                (LiveStreamDetails::Uploaded, duration?, false)
            };

        Ok(VideoExtras {
            time_string: time_string,
            category_id: snippet.category_id.ok_or(MissingContent::CategoryId)?,
            live_stream_details: live_stream_details,
            is_short: is_short(v.id.ok_or(MissingContent::VideoId)?.as_str()).await?,
            is_scheduled: is_scheduled,
        })
    }))
    .await
}

#[derive(Debug)]
#[allow(dead_code)]
pub enum InitializeCategoriesError {
    MissingContent(MissingContent),
    YouTube3(google_youtube3::Error),
}

impl From<MissingContent> for InitializeCategoriesError {
    fn from(value: MissingContent) -> Self {
        Self::MissingContent(value)
    }
}

impl From<google_youtube3::Error> for InitializeCategoriesError {
    fn from(value: google_youtube3::Error) -> Self {
        Self::YouTube3(value)
    }
}

pub async fn initialize_categories() -> Result<CategoryCache, InitializeCategoriesError> {
    let response = YOUTUBE
        .get()
        .unwrap()
        .use_with(|yt| async move {
            yt.video_categories()
                .list(&vec!["snippet".into()])
                .region_code("US")
                .hl("en_US")
                .param("key", KEY.get().unwrap())
                .doit()
                .await
        })
        .await?
        .1;

    let Some(video_categories) = response.items else {
        return Err(MissingContent::VideoCategories)?;
    };

    let uhhh = video_categories
        .into_iter()
        .filter_map(|vc| match (vc.id, vc.snippet.and_then(|s| s.title)) {
            (Some(id), Some(s)) => Some((id, s)),
            _ => None,
        })
        .collect::<HashMap<_, _>>();

    Ok(CategoryCache { dict: uhhh })
}

const CATEGORY_EMOJI: [(&str, &str); 31] = [
    ("Film & Animation", "🎞️"),
    ("Autos & Vehicles", "🚗"),
    ("Music", "🎶"),
    ("Pets & Animals", "🐈"),
    ("Sports", "⚽"),
    ("Short Movies", "📹"),
    ("Travel & Events", "🗺️"),
    ("Gaming", "🎮"),
    ("Videoblogging", "🤳"),
    ("People & Blogs", "📓"),
    ("Comedy", "😂"),
    ("Entertainment", "🎭"),
    ("News & Politics", "🗞️"),
    ("Howto & Style", "🧤"),
    ("Education", "🎓"),
    ("Science & Technology", "📡"),
    ("Nonprofits & Activism", "📢"),
    ("Movies", "📼"),
    ("Anime/Animation", "✨"),
    ("Action/Adventure", "🚵"),
    ("Classics", "🎼"),
    ("Documentary", "🔍"),
    ("Drama", "🤬"),
    ("Family", "👪"),
    ("Foreign", "🏝️"),
    ("Horror", "👻"),
    ("Sci-Fi/Fantasy", "🔮"),
    ("Thriller", "😰"),
    ("Shorts", "📱"),
    ("Shows", "📺"),
    ("Trailers", "🎬"),
];

#[allow(dead_code)]
pub enum CategoryTitleError {
    MissingContent(MissingContent),
    YouTube3(google_youtube3::Error),
}

impl From<MissingContent> for CategoryTitleError {
    fn from(value: MissingContent) -> Self {
        Self::MissingContent(value)
    }
}

impl From<google_youtube3::Error> for CategoryTitleError {
    fn from(value: google_youtube3::Error) -> Self {
        Self::YouTube3(value)
    }
}

pub struct CategoryCache {
    dict: HashMap<String, String>,
}

impl CategoryCache {
    pub async fn get(
        &mut self,
        id: String,
    ) -> Result<(&str, Option<&'static str>), CategoryTitleError> {
        if !self.dict.contains_key(&id) {
            let _id = id.clone();

            let response = YOUTUBE
                .get()
                .unwrap()
                .use_with(|yt| async move {
                    yt.video_categories()
                        .list(&vec!["snippet".into()])
                        .add_id(_id.as_str())
                        .hl("en_US")
                        .param("key", KEY.get().unwrap())
                        .doit()
                        .await
                })
                .await?
                .1;

            let Some(video_categories) = response.items else {
                return Err(MissingContent::VideoCategories)?;
            };

            for vc in video_categories {
                let _id = match vc.id {
                    Some(_id) if _id == id => _id,
                    _ => continue,
                };
                let title = vc
                    .snippet
                    .and_then(|s| s.title)
                    .ok_or(MissingContent::VideoCategoryTitle)?;
                self.dict.insert(_id, title);
            }
        }

        let title = self
            .dict
            .get(&id)
            .ok_or(MissingContent::VideoCategoryTitle)?
            .as_str();

        let emoji = CATEGORY_EMOJI
            .iter()
            .find(|(t, _)| *t == title)
            .map(|(_, s)| *s);

        Ok((title, emoji))
    }
}
