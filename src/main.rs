// based on https://github.com/serenity-rs/serenity/blob/current/examples/e14_slash_commands/src/main.rs
// you **shouldn't** need to modify this file at all, unless you want to use an interaction other than commands and components.
// in that case, modify interaction_create below and create a separate module for it in another file.

mod commands;
mod components;
mod db;
mod generate_components;
mod rate_limit;
mod update_loop;
mod youtube;

use commands::*;
use components::*;

use db::update_db_schema;
use google_youtube3::client::NoToken;
use google_youtube3::{YouTube, hyper, hyper_rustls};

use hyper::client::HttpConnector;
use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};

use sqlx::migrate::MigrateDatabase;
use sqlx::{Sqlite, SqlitePool, query};
use update_loop::update_loop;
use youtube::{CategoryCache, initialize_categories};

use std::env;
use std::sync::LazyLock;
use std::time::Duration;

use thiserror::Error;

use tokio::sync::Mutex;
// use tokio::sync::mpsc;

use serenity::all::{Context, EventHandler, GatewayIntents};
use serenity::async_trait;
use serenity::model::application::{Command, Interaction};
use serenity::model::gateway::Ready;
use serenity::model::id::UserId;

use config::{Config, ConfigError, File};

use crate::rate_limit::RateLimiter;

static ADMIN_USERS: LazyLock<Vec<UserId>> = LazyLock::new(|| {
    CONFIG
        .get_array("admins")
        .expect("Error getting admins array from config")
        .iter()
        .map(|val| {
            UserId::new(
                val.clone()
                    .into_uint()
                    .expect("Failed to parse admin list entry into UserId"),
            )
        })
        .collect::<Vec<UserId>>()
});

// If you put `use crate::CONFIG;` in another file, it will include this, and you will have access to the raw config values for your own use.
static CONFIG: LazyLock<Config> = LazyLock::new(|| build_config().unwrap());

const DB_URL: &str = "sqlite://sqlite.db";

static DB: async_lazy::Lazy<SqlitePool> = async_lazy::Lazy::new(|| {
    Box::pin(async {
        // based on https://tms-dev-blog.com/rust-sqlx-basics-with-sqlite/#Creating_an_SQLite_database, accessed 2024-08-20.
        if !Sqlite::database_exists(DB_URL).await.unwrap() {
            println!("Creating DB files.");
            Sqlite::create_database(DB_URL).await.unwrap();
            let db = SqlitePool::connect(DB_URL).await.unwrap();
            query(
                "CREATE TABLE IF NOT EXISTS channels (
                playlist_id TEXT NOT NULL,
                channel_id INTEGER NOT NULL,
                most_recent TEXT NOT NULL CHECK ( DATETIME(most_recent) IS most_recent ),
                PRIMARY KEY (playlist_id, channel_id)
            ) STRICT",
            )
            .execute(&db)
            .await
            .unwrap();
            db
        } else {
            SqlitePool::connect(DB_URL).await.unwrap()
        }
    })
});

static HYPER: LazyLock<hyper::Client<HttpsConnector<HttpConnector>>> = LazyLock::new(|| {
    hyper::Client::builder().build(
        HttpsConnectorBuilder::new()
            .with_native_roots()
            .unwrap()
            .https_or_http()
            .enable_http2()
            .build(),
    )
});

static KEY: LazyLock<Box<str>> = LazyLock::new(|| {
    CONFIG
        .get_string("key")
        .expect(
            "YouTube Data API key not found. Either:\n
- put it in the `config` file (key = \"key\")\n
- set environment variable YOUTUBE_KEY.\n",
        )
        .into_boxed_str()
});

static REGION_CODE: LazyLock<Box<str>> =
    LazyLock::new(|| CONFIG.get_string("region_code").unwrap().into_boxed_str());
static LANGUAGE: LazyLock<Box<str>> =
    LazyLock::new(|| CONFIG.get_string("language").unwrap().into_boxed_str());

static YOUTUBE: LazyLock<RateLimiter<YouTube<HttpsConnector<HttpConnector>>>> =
    LazyLock::new(|| {
        let youtube = YouTube::new(HYPER.clone(), NoToken);
        RateLimiter::new(TIME_PER_REQUEST, youtube)
    });

static CATEGORY_TITLES: async_lazy::Lazy<Mutex<CategoryCache>> = async_lazy::Lazy::new(|| {
    Box::pin(async { Mutex::new(initialize_categories().await.unwrap()) })
});

// 1 day / 10,000 (which is the rate limit)
const TIME_PER_REQUEST: Duration = Duration::from_millis(
    1000 // 1000 milliseconds per second
    * 60 // 60 seconds per minute
    * 60 // 60 minutes per hour
    * 24 // 24 hours per day
    / 10000, // 10000 requests per day
);

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        match interaction {
            Interaction::Command(command) => {
                // Commands are implemented in src/commands.rs
                if let Err(why) = handle_command(ctx, command).await {
                    println!("Cannot respond to slash command: {}", why);
                };
            }
            Interaction::Component(component) => {
                // Components are implemented in src/components.rs
                if let Err(why) = handle_component(ctx, component).await {
                    println!("Cannot respond to message component: {}", why);
                }
            }
            _ => println!("Unimplemented interaction: {:?}", interaction.kind()),
        }
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);

        Command::set_global_commands(&ctx.http, create_commands())
            .await
            .expect("Failed to set application commands");

        tokio::spawn(update_loop(ctx.http));
    }
}

fn build_config() -> Result<Config, ConfigError> {
    Config::builder()
        .add_source(File::with_name("config"))
        .set_default("admins", Vec::<u64>::new())?
        .set_override_option("token", env::var("DISCORD_TOKEN").ok())?
        .set_override_option("key", env::var("YOUTUBE_KEY").ok())?
        .set_default("region_code", "US")?
        .set_default("language", "en_US")?
        .build()
}

#[derive(Error, Debug)]
pub enum MainError {
    #[error("Sqlx({0})")]
    Sqlx(#[from] sqlx::Error),
    #[error("Config({0})")]
    Config(#[from] ConfigError),
    #[error("Serenity({0})")]
    Serenity(#[from] serenity::Error),
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), MainError> {
    update_db_schema().await?;

    let token = CONFIG.get_string("token").expect("Token not found. Either:\n
                                                                    - put it in the `config` file (token = \"token\")\n
                                                                    - set environment variable DISCORD_TOKEN.\n");

    if ADMIN_USERS.is_empty() {
        println!(
            "\tWARNING: No admin users specified in config file!\n\tBy default, any user will be able to shut down your bot."
        );
    }

    // Build our client.
    let mut client = serenity::Client::builder(token, GatewayIntents::empty())
        .event_handler(Handler)
        .await?;

    // Channel for the shutdown command to use later
    // let (sender, mut receiver) = mpsc::channel(64);
    // SHUTDOWN_SENDER.set(sender).unwrap();

    // let shard_manager = client.shard_manager.clone();

    // // Spawns a task that waits for the shutdown command, then shuts down the bot.
    // tokio::spawn(async move {
    //     loop {
    //         // I have left open the possibility of using b=false for something "softer" in case you need it.
    //         let b = receiver.recv().await.expect("Shutdown message pass error");
    //         if b {
    //             shard_manager.shutdown_all().await;
    //             println!("Shutdown shard manager");
    //             break;
    //         }
    //     }
    // });

    // Start the client.

    client.start().await?;

    println!("Client shutdown cleanly");

    Ok(())
}
