use crate::db::{get_channels_to_send, get_playlists, update_most_recent};
use crate::youtube::{get_uploads_from_playlist, UploadsError, Video};

use std::collections::VecDeque;
use std::time::Duration;

use serenity::all::{CacheHttp, ChannelId, CreateMessage, MessageFlags};

use tokio::time::sleep;

struct Workunit<'a> {
    playlist_id: &'a String,
    video: Video,
    channel_id: ChannelId,
}

async fn get_workunits<'a>(playlists: &'a Vec<String>) -> Vec<Workunit<'a>> {
    let mut workunits: Vec<Workunit> = vec![];
    for playlist_id in playlists.iter() {
        let mut videos = match get_uploads_from_playlist(&playlist_id).await {
            Ok(v) => v,

            Err(UploadsError::MissingContent(mc)) => {
                println!("get_uploads_from_playlist in update_loop:\t{:?}", mc);
                continue;
            }
            Err(UploadsError::YouTube3(e)) => {
                println!("get_uploads_from_playlist in update_loop:\t{}", e);
                continue;
            }
        };

        videos.reverse();
        for video in videos {
            let channels = match get_channels_to_send(&playlist_id, &video.published_at).await {
                Ok(v) => v,

                Err(e) => {
                    println!("get_channels_to_send in update_loop:\t{}", e);
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
    }
    workunits
}

async fn do_workunits<'a>(
    workunits: Vec<Workunit<'a>>,
    reduced_duration: Duration,
    http: impl CacheHttp,
) {
    let mut db_retries = VecDeque::new();
    for w in workunits {
        sleep(reduced_duration).await;

        if let Err(e) = w
            .channel_id
            .send_message(
                &http,
                CreateMessage::new()
                    .content(format!("https://youtu.be/{}", w.video.id))
                    .flags(MessageFlags::empty()),
            )
            .await
        {
            println!("send_message in update_loop:\t{}", e);
            continue;
        }

        if let Err(e) =
            update_most_recent(w.playlist_id, &w.channel_id, &w.video.published_at).await
        {
            println!(
                "update_most_recent in update_loop:\t{}\n
                        DB in illegal state, will be fixed later.",
                e
            );
            db_retries.push_back(w);
        }
    }

    resync_db(db_retries, reduced_duration).await
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
pub async fn update_loop(sleep_seconds: u64, http: impl CacheHttp) {
    let duration = Duration::from_secs(sleep_seconds);
    loop {
        let playlists = match get_playlists().await {
            Ok(v) => v,

            Err(e) => {
                println!("get_playlists in update_loop:\t{}", e);
                sleep(duration).await;
                continue;
            }
        };

        let playlists_len = playlists.len();
        if playlists_len == 0 {
            sleep(duration).await;
            continue;
        }

        let workunits = get_workunits(&playlists).await;

        let reduced_duration = match workunits.len().try_into() {
            Ok(v) => match duration.checked_div(v) {
                Some(d) => d,
                None => {
                    // no workunits
                    sleep(duration).await;
                    continue;
                }
            },
            Err(_) => {
                println!("Insane amount of workunits!!! {}", workunits.len());
                // if there are more than 2^32 workunits to get through... maybe sleeping isn't such a good idea.
                Duration::ZERO
            }
        };

        do_workunits(workunits, reduced_duration, &http).await
    }
}
