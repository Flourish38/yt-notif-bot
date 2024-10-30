use crate::db::{get_channels_to_send, get_playlists, update_most_recent};
use crate::youtube::{get_uploads_from_playlist, get_videos_durations, UploadsError, Video};
use crate::TIME_PER_REQUEST;

use std::collections::VecDeque;
use std::time::Duration;

use serenity::all::{CacheHttp, ChannelId, CreateMessage, Message, MessageFlags};

use tokio::time::sleep;

struct IndexWorkunit<'a> {
    playlist_id: &'a String,
    index: usize,
    channel_id: ChannelId,
}

struct Workunit<'a> {
    playlist_id: &'a String,
    video: Video,
    duration: String,
    channel_id: ChannelId,
}

async fn process_playlists<'a>(playlists: &'a Vec<String>, http: impl CacheHttp) -> () {
    for playlist_id in playlists.iter() {
        let mut videos = match get_uploads_from_playlist(&playlist_id).await {
            Ok(v) => v,

            Err(UploadsError::MissingContent(mc)) => {
                println!("get_uploads_from_playlist in process_playlists:\t{:?}", mc);
                sleep(TIME_PER_REQUEST).await;
                continue;
            }
            Err(UploadsError::YouTube3(e)) => {
                println!("get_uploads_from_playlist in process_playlists:\t{}", e);
                sleep(TIME_PER_REQUEST).await;
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
            assign_workunit_duration(videos_slice, index_workunits, first_index, &http).await;
        } else {
            sleep(TIME_PER_REQUEST).await;
        }
    }
}

async fn assign_workunit_duration<'a>(
    videos: &[Video],
    index_workunits: Vec<IndexWorkunit<'a>>,
    first_index: usize,
    http: &impl CacheHttp,
) {
    sleep(TIME_PER_REQUEST).await; // sleep because we're making another api request, no getting rate-limited!
    let durations = match get_videos_durations(videos).await {
        Ok(v) => v,
        Err(e) => {
            println!("get_videos_durations in assign_workunit_duration:\t{:?}", e);
            sleep(TIME_PER_REQUEST).await;
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
                duration: durations[index].clone(),
                channel_id: iw.channel_id,
            }
        })
        .collect();

    do_workunits(workunits, http).await
}

fn reduce_duration(duration: Duration, n: usize) -> Option<Duration> {
    match n.try_into() {
        Ok(v) => duration.checked_div(v),
        Err(_) => {
            println!("Insane amount of workunits!!! {}", n);
            // if there are more than 2^32 workunits to get through... maybe sleeping isn't such a good idea.
            Some(Duration::ZERO)
        }
    }
}

async fn do_workunits<'a>(workunits: Vec<Workunit<'a>>, http: impl CacheHttp) {
    let reduced_duration = match reduce_duration(TIME_PER_REQUEST, workunits.len()) {
        Some(d) => d,
        None => {
            // There must be zero workunits, sleep to avoid requesting too quickly
            sleep(TIME_PER_REQUEST).await;
            return;
        }
    };

    let mut db_retries = VecDeque::new();
    for w in workunits {
        let msg = match w
            .channel_id
            .send_message(
                &http,
                CreateMessage::new()
                    .content(format!(
                        "https://youtu.be/{} `({})`",
                        w.video.id, w.duration
                    ))
                    .flags(MessageFlags::empty()),
            )
            .await
        {
            Err(e) => {
                println!("send_message in do_workunits:\t{}", e);
                sleep(reduced_duration).await;
                continue;
            }
            Ok(msg) => msg,
        };

        update_db_entry(&mut db_retries, w, msg, &http).await;
        sleep(reduced_duration).await;
    }

    resync_db(db_retries, reduced_duration).await
}

async fn update_db_entry<'a>(
    db_retries: &mut VecDeque<Workunit<'a>>,
    w: Workunit<'a>,
    msg: Message,
    http: impl CacheHttp,
) {
    if let Err(e) = update_most_recent(w.playlist_id, &w.channel_id, &w.video.published_at).await {
        println!(
            "update_most_recent in update_db_entry:\t{}\n
            Attempting to delete message to regain consistency...",
            e
        );
        if let Err(e) = msg.delete(http).await {
            println!(
                "msg.delete in update_db_entry:\t{}\n
                Uh oh. Adding to queue to be reprocessed later.",
                e
            );
            db_retries.push_back(w);
        }
    }
}

async fn resync_db<'a>(mut db_retries: VecDeque<Workunit<'a>>, reduced_duration: Duration) {
    if db_retries.len() != 0 {
        println!("{} DB update failures to resolve", db_retries.len());
        let mut failure_count: usize = 0;
        loop {
            match db_retries.pop_front() {
                None => break,
                Some(w) => {
                    if let Err(_) =
                        update_most_recent(w.playlist_id, &w.channel_id, &w.video.published_at)
                            .await
                    {
                        failure_count += 1;
                        db_retries.push_back(w);
                    }
                    // at least make an attempt not to throttle the entire system
                    sleep(reduced_duration).await;
                }
            }
        }
        println!(
            "All failures resolved after {} additional failures.",
            failure_count
        );
    }
}

// This function is ugly, but not terribly complicated.
// Just lots, and lots, of error handling.
pub async fn update_loop(http: impl CacheHttp) {
    loop {
        let playlists = match get_playlists().await {
            Ok(v) => v,

            Err(e) => {
                println!("get_playlists in update_loop:\t{}", e);
                sleep(TIME_PER_REQUEST).await;
                continue;
            }
        };

        if playlists.len() == 0 {
            sleep(TIME_PER_REQUEST).await;
            continue;
        }

        process_playlists(&playlists, &http).await;
    }
}
