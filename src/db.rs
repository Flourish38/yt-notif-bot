use crate::DB;

use google_youtube3::chrono::{DateTime, SecondsFormat, Utc};
use serenity::all::ChannelId;
use sqlx::{query, sqlite::SqliteQueryResult};

fn into_sqlite(dt: DateTime<Utc>) -> String {
    dt.to_rfc3339_opts(SecondsFormat::Secs, true)
        // format to work with sqlite's DATETIME() function
        .trim_end_matches('Z')
        .replace('T', " ")
}

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
    .bind(into_sqlite(Utc::now()))
    .execute(DB.get().unwrap())
    .await
}
