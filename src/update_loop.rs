use crate::db::{get_channels_to_send, get_playlists, update_most_recent};
use crate::youtube::{get_uploads_from_playlist, UploadsError, Video};

use std::collections::VecDeque;
use std::time::Duration;

use serenity::all::{CacheHttp, ChannelId, CreateMessage, Message, MessageFlags};

use tokio::time::sleep;

struct Workunit<'a> {
    playlist_id: &'a String,
    video: Video,
    channel_id: ChannelId,
}

async fn process_playlists<'a>(
    playlists: &'a Vec<String>,
    duration: Duration,
    http: impl CacheHttp,
) -> () {
    for playlist_id in playlists.iter() {
        let mut workunits: Vec<Workunit> = vec![];
        let mut videos = match get_uploads_from_playlist(&playlist_id).await {
            Ok(v) => v,

            Err(UploadsError::MissingContent(mc)) => {
                println!("get_uploads_from_playlist in process_playlists:\t{:?}", mc);
                sleep(duration).await;
                continue;
            }
            Err(UploadsError::YouTube3(e)) => {
                println!("get_uploads_from_playlist in process_playlists:\t{}", e);
                sleep(duration).await;
                continue;
            }
        };

        videos.reverse();
        for video in videos {
            let channels = match get_channels_to_send(&playlist_id, &video.published_at).await {
                Ok(v) => v,

                Err(e) => {
                    println!("get_channels_to_send in process_playlists:\t{}", e);
                    continue;
                }
            };

            for channel in channels {
                workunits.push(Workunit {
                    playlist_id: playlist_id,
                    video: video.clone(),
                    channel_id: channel,
                })
            }
        }

        do_workunits(workunits, duration, &http).await;
    }
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

async fn do_workunits<'a>(workunits: Vec<Workunit<'a>>, duration: Duration, http: impl CacheHttp) {
    let reduced_duration = match reduce_duration(duration, workunits.len()) {
        Some(d) => d,
        None => {
            sleep(duration).await;
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
                    .content(format!("https://youtu.be/{}", w.video.id))
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

    resync_db(db_retries, duration).await
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
    // 1 day / 10,000 (which is the rate limit)
    let duration = Duration::from_secs(
        60 // 60 seconds per minute
        * 60 // 60 minutes per hour
        * 24, // 24 hours per day
    ) / 10000;
    loop {
        let playlists = match get_playlists().await {
            Ok(v) => v,

            Err(e) => {
                println!("get_playlists in update_loop:\t{}", e);
                sleep(duration).await;
                continue;
            }
        };

        if playlists.len() == 0 {
            sleep(duration).await;
            continue;
        }

        process_playlists(&playlists, duration, &http).await;
    }
}
