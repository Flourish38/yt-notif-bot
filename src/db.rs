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
    playlist_id: &String,
    channel_id: ChannelId,
) -> Result<SqliteQueryResult, sqlx::Error> {
    query(
        "INSERT INTO channels (playlist_id, channel_id, most_recent)
            VALUES ($1, $2, $3)",
    )
    .bind(playlist_id)
    .bind(channel_id.get() as i64)
    .bind(into_sqlite(&Utc::now()))
    .execute(DB.get().unwrap())
    .await
}

// u32 is technically the incorrect type, but it makes for one less potential conversion error in howmany_command.
// Also, in order for that to be an issue, you would need so many playlists that it would be 1176 years before you check the same one twice.
pub async fn get_num_playlists() -> Result<u32, sqlx::Error> {
    query(
        "SELECT COUNT(DISTINCT playlist_id) playlist_id 
            FROM channels",
    )
    .fetch_one(DB.get().unwrap())
    .await?
    .try_get(0)
}

pub async fn get_playlists() -> Result<Vec<String>, sqlx::Error> {
    query(
        "SELECT DISTINCT playlist_id 
            FROM channels 
            ORDER BY playlist_id",
    )
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

pub async fn update_most_recent(
    playlist_id: &String,
    channel_id: &ChannelId,
    new_value: &DateTime<Utc>,
) -> Result<SqliteQueryResult, sqlx::Error> {
    query(
        "UPDATE channels
            SET most_recent = $1
            WHERE playlist_id == $2
            AND channel_id == $3",
    )
    .bind(into_sqlite(new_value))
    .bind(playlist_id)
    .bind(channel_id.get() as i64)
    .execute(DB.get().unwrap())
    .await
}

pub async fn delete_channel(
    playlist_id: &String,
    channel_id: ChannelId,
) -> Result<SqliteQueryResult, sqlx::Error> {
    query(
        "DELETE FROM channels
            WHERE playlist_id == $1
            AND channel_id == $2",
    )
    .bind(playlist_id)
    .bind(channel_id.get() as i64)
    .execute(DB.get().unwrap())
    .await
}

pub async fn update_db_schema() -> Result<Option<SqliteQueryResult>, sqlx::Error> {
    let db = DB.get().unwrap();

    let user_version: i32 = query("PRAGMA user_version")
        .fetch_one(db)
        .await?
        .try_get(0)?;

    match user_version {
        0 => query(
            "ALTER TABLE channels
            ADD COLUMN live_allowed INTEGER NOT NULL CHECK (live_allowed IN (0, 1)) DEFAULT FALSE;
            ALTER TABLE channels
            ADD COLUMN vod_allowed INTEGER NOT NULL CHECK (vod_allowed IN (0, 1)) DEFAULT FALSE;
            ALTER TABLE channels
            ADD COLUMN short_allowed INTEGER NOT NULL CHECK (short_allowed IN (0, 1)) DEFAULT TRUE;
            PRAGMA user_version = 1;",
        )
        .execute(db)
        .await
        .map(Some),
        1 => Ok(None),
        n => panic!("Unknown user_version: {}", n),
    }
}
