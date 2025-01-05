use crate::db::{get_channels_to_send, get_playlists, update_most_recent};
use crate::youtube::{
    get_uploads_from_playlist, get_videos_extras, LiveStreamDetails, UploadsError, Video,
    VideoExtras,
};

use std::collections::VecDeque;

use serenity::all::{CacheHttp, ChannelId, CreateMessage, Message, MessageFlags};

struct IndexWorkunit<'a> {
    playlist_id: &'a String,
    index: usize,
    channel_id: ChannelId,
}

struct Workunit<'a> {
    playlist_id: &'a String,
    video: Video,
    extras: VideoExtras,
    channel_id: ChannelId,
}

impl<'a> Workunit<'a> {
    async fn send_message(&self, http: impl CacheHttp) -> Result<Option<Message>, serenity::Error> {
        if !matches!(self.extras.live_stream_details, LiveStreamDetails::Uploaded) {
            return Ok(None);
        }

        let msg_text = format!(
            "https://youtu.be/{} {}`({})`\n```\ncategoryId: {}\ntags: [{}]\n```",
            self.video.id,
            match self.extras.live_stream_details {
                // Currently, this does nothing.
                LiveStreamDetails::Live => "🔴 ",
                LiveStreamDetails::VOD => "⭕ ",
                LiveStreamDetails::Uploaded => "",
            },
            self.extras.duration,
            self.extras.category_id,
            self.extras.tags.join(",")
        );
        self.channel_id
            .send_message(
                &http,
                CreateMessage::new()
                    .content(msg_text)
                    .flags(MessageFlags::empty()),
            )
            .await
            .map(Some)
    }
}

async fn process_playlists<'a>(playlists: &'a Vec<String>, http: impl CacheHttp) -> () {
    for playlist_id in playlists.iter() {
        let mut videos = match get_uploads_from_playlist(&playlist_id).await {
            Ok(v) => v,

            Err(UploadsError::MissingContent(mc)) => {
                println!("get_uploads_from_playlist in process_playlists:\t{:?}", mc);
                continue;
            }
            Err(UploadsError::YouTube3(e)) => {
                println!("get_uploads_from_playlist in process_playlists:\t{}", e);
                continue;
            }
        };

        videos.reverse();

        let mut first_index = 0;
        let mut index_workunits: Vec<IndexWorkunit> = vec![];
        for (i, video) in videos.iter().enumerate() {
            let channels = match get_channels_to_send(&playlist_id, &video.published_at).await {
                Ok(v) => v,

                Err(e) => {
                    println!("get_channels_to_send in process_playlists:\t{}", e);
                    continue;
                }
            };

            if channels.len() == 0 {
                if first_index == i {
                    // This if statement only doesn't happen if the videos are not returned in upload order.
                    // That should never happen, but better safe than sorry.
                    first_index = i + 1;
                }
            } else {
                for channel in channels {
                    index_workunits.push(IndexWorkunit {
                        playlist_id: playlist_id,
                        index: i,
                        channel_id: channel,
                    })
                }
            }
        }

        let videos_slice = &videos[first_index..];

        if videos_slice.len() != 0 {
            assign_workunit_extras(videos_slice, index_workunits, first_index, &http).await;
        }
    }
}

async fn assign_workunit_extras<'a>(
    videos: &[Video],
    index_workunits: Vec<IndexWorkunit<'a>>,
    first_index: usize,
    http: &impl CacheHttp,
) {
    let extras = match get_videos_extras(videos).await {
        Ok(v) => v,
        Err(e) => {
            println!("get_videos_extras in assign_workunit_duration:\t{:?}", e);
            return;
        }
    };

    let workunits = index_workunits
        .into_iter()
        .map(|iw| {
            let index = iw.index - first_index;
            Workunit {
                playlist_id: iw.playlist_id,
                video: videos[index].clone(),
                extras: extras[index].clone(),
                channel_id: iw.channel_id,
            }
        })
        .collect();

    do_workunits(workunits, http).await
}

async fn do_workunits<'a>(workunits: Vec<Workunit<'a>>, http: impl CacheHttp) {
    let mut db_retries = VecDeque::new();
    for w in workunits {
        let msg = match w.send_message(&http).await {
            Err(e) => {
                println!("send_message in do_workunits:\t{}", e);
                continue;
            }
            Ok(msg) => msg,
        };

        update_db_entry(&mut db_retries, w, msg, &http).await;
    }

    resync_db(db_retries).await
}

async fn update_db_entry<'a>(
    db_retries: &mut VecDeque<Workunit<'a>>,
    w: Workunit<'a>,
    o_msg: Option<Message>,
    http: impl CacheHttp,
) {
    let Err(e1) = update_most_recent(w.playlist_id, &w.channel_id, &w.video.published_at).await
    else {
        return;
    };

    println!("update_most_recent in update_db_entry:\t{}", e1);
    let Some(msg) = o_msg else {
        return; // No message that could be mistakenly sent twice, so no big deal
    };

    println!("Attempting to delete message to restore consistency...");
    let Err(e2) = msg.delete(http).await else {
        return; // Message deleted successfully, consistency restored
    };

    println!(
        "msg.delete in update_db_entry:\t{}\nUh oh. Adding to queue to be reprocessed later.",
        e2
    );
    db_retries.push_back(w);
}

// This function is a brute force way of making sure that the same message doesn't get sent twice.
// This blocks the entire update loop until the database is back in sync.
//
// The only way that this function should ever possibly be called with a non-empty queue is if
//     1. The database fails to write (should be almost impossible, given that sqlite is a local file system)
// AND 2. Connection is somehow lost with discord immediately after successfully sending the message, preventing it from being deleted.
//
// It'd better not.
async fn resync_db<'a>(mut db_retries: VecDeque<Workunit<'a>>) {
    if db_retries.len() == 0 {
        return;
    }

    println!("{} DB update failures to resolve", db_retries.len());
    let mut failure_count: usize = 0;
    loop {
        let Some(w) = db_retries.pop_front() else {
            break; // No workunits left, thank goodness!
        };

        if let Err(_) =
            update_most_recent(w.playlist_id, &w.channel_id, &w.video.published_at).await
        {
            failure_count += 1;
            db_retries.push_back(w);
        }

        tokio::time::sleep(std::time::Duration::from_millis(5)).await; // at least attempt not to throttle the system
    }
    println!(
        "All failures resolved after {} additional failures.",
        failure_count
    );
}

// This function is ugly, but not terribly complicated.
// Just lots, and lots, of error handling.
pub async fn update_loop(http: impl CacheHttp) {
    loop {
        let playlists = match get_playlists().await {
            Ok(v) => v,

            Err(e) => {
                println!("get_playlists in update_loop:\t{}", e);
                continue;
            }
        };

        if playlists.len() == 0 {
            continue;
        }

        process_playlists(&playlists, &http).await;
    }
}
