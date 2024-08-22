use crate::DB;

use google_youtube3::chrono::{SecondsFormat, Utc};
use serenity::all::ChannelId;
use sqlx::{query, sqlite::SqliteQueryResult};

pub async fn add_channel(
    playlist: &String,
    channel: ChannelId,
) -> Result<SqliteQueryResult, sqlx::Error> {
    query(
        "INSERT INTO channels (playlist_id, channel_id, most_recent)
        VALUES ($1, $2, $3)",
    )
    .bind(playlist)
    .bind(channel.get() as i64)
    .bind(
        Utc::now()
            .to_rfc3339_opts(SecondsFormat::Secs, true)
            // format to work with sqlite's DATETIME() function
            .replace('T', " ")
            .trim_end_matches('Z'),
    )
    .execute(DB.get().unwrap())
    .await
}
