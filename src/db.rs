use crate::DB;

use google_youtube3::chrono::{DateTime, SecondsFormat, Utc};
use serenity::all::ChannelId;
use sqlx::{query, sqlite::SqliteQueryResult, Row};

fn into_sqlite(dt: &DateTime<Utc>) -> String {
    dt.to_rfc3339_opts(SecondsFormat::Secs, true)
        // format to work with sqlite's DATETIME() function
        .trim_end_matches('Z')
        .replace('T', " ")
}

#[allow(dead_code)]
fn from_sqlite(str: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(&format!("{}Z", str))
        .unwrap()
        .into()
}

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
    .bind(into_sqlite(&DateTime::default()))
    .execute(DB.get().unwrap())
    .await
}

pub async fn get_playlists() -> Result<Vec<String>, sqlx::Error> {
    query("SELECT DISTINCT playlist_id FROM channels")
        .fetch_all(DB.get().unwrap())
        .await?
        .into_iter()
        .map(|s| s.try_get(0))
        .collect()
}

pub async fn get_channels_to_send(
    playlist_id: &String,
    published_at: &DateTime<Utc>,
) -> Result<Vec<ChannelId>, sqlx::Error> {
    query(
        "SELECT DISTINCT channel_id 
    FROM channels
    WHERE playlist_id == $1
    AND most_recent < $2",
    )
    .bind(playlist_id)
    .bind(into_sqlite(published_at))
    .fetch_all(DB.get().unwrap())
    .await
    .unwrap()
    .into_iter()
    .map(|s| Ok(ChannelId::new(s.try_get(0)?)))
    .collect()
}
