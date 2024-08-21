// based on https://github.com/serenity-rs/serenity/blob/current/examples/e14_slash_commands/src/main.rs
// you **shouldn't** need to modify this file at all, unless you want to use an interaction other than commands and components.
// in that case, modify interaction_create below and create a separate module for it in another file.

mod commands;
mod components;
mod generate_components;

use commands::*;
use components::*;

use std::env;

use tokio::sync::{mpsc, OnceCell};

use serenity::all::{Context, EventHandler, GatewayIntents};
use serenity::model::application::{Command, Interaction};
use serenity::model::gateway::Ready;
use serenity::model::id::UserId;
use serenity::{async_trait, Client};

use config::{Config, ConfigError, File};

// Technically this initial vec is never used but it makes it so you don't need to use an expect() whenever you use the variable.
// Also, according to the docs, vecs of size 0 don't allocate any memory anyways, so it literally doesn't matter.

static ADMIN_USERS: OnceCell<Vec<UserId>> = OnceCell::const_new();

// Unused by default, but useful in case you need it.
// If you put `use crate::CONFIG;` in another file, it will include this, and you will have access to the raw config values for your own use.
static CONFIG: OnceCell<Config> = OnceCell::const_new();

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
    }
}

fn build_config() -> Result<Config, ConfigError> {
    Config::builder()
        .add_source(File::with_name("config"))
        .set_default("admins", Vec::<u64>::new())?
        .set_override_option("token", env::var("DISCORD_TOKEN").ok())?
        .build()
}

#[tokio::main]
async fn main() {
    // Configure the client with your Discord bot token in your `config` file.
    let config = build_config().expect("Config failed");

    let token = config.get_string("token").expect("Token not found. Either:\n
                                                                    - put it in the `config` file (token = \"token\")\n
                                                                    - set environment variable DISCORD_TOKEN.\n");

    let admins = config
        .get_array("admins")
        .expect("Somehow failed to get admin list even though there is a default value??")
        .iter()
        .map(|val| {
            UserId::new(
                val.clone()
                    .into_uint()
                    .expect("Failed to parse admin list entry into UserId"),
            )
        })
        .collect::<Vec<UserId>>();

    if admins.is_empty() {
        println!("\tWARNING: No admin users specified in config file!\n\tBy default, any user will be able to shut down your bot.");
    }

    ADMIN_USERS
        .set(admins)
        .expect("Somehow a race condition for ADMIN_USERS???");

    CONFIG
        .set(config)
        .expect("Somehow a race condition for CONFIG???");

    // Build our client.
    let mut client = Client::builder(token, GatewayIntents::empty())
        .event_handler(Handler)
        .await
        .expect("Error creating client");

    // Channel for the shutdown command to use later
    let (sender, mut receiver) = mpsc::channel(64);
    SHUTDOWN_SENDER
        .set(sender)
        .expect("Somehow a race condition for SHUTDOWN_SENDER???");

    let shard_manager = client.shard_manager.clone();

    // Spawns a task that waits for the shutdown command, then shuts down the bot.
    tokio::spawn(async move {
        loop {
            // I have left open the possibility of using b=false for something "softer" in case you need it.
            let b = receiver.recv().await.expect("Shutdown message pass error");
            if b {
                shard_manager.shutdown_all().await;
                println!("Shutdown shard manager");
                break;
            }
        }
    });

    // Start the client.
    match client.start().await {
        Err(why) => println!("Client error: {}", why),
        Ok(_) => println!("Client shutdown cleanly"),
    }
}
