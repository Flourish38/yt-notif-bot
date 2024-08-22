use crate::db::{add_channel, get_channels_to_send, get_playlists};
use crate::generate_components::make_button;
use crate::youtube::{
    get_upload_playlist_id, get_uploads_from_playlist, PlaylistIdError, UploadsError,
};
use crate::ADMIN_USERS;

use std::time::{Duration, Instant};

use serenity::all::{
    CacheHttp, ChannelId, CommandInteraction, CommandOptionType, Context, CreateActionRow,
    CreateCommand, CreateCommandOption, CreateInteractionResponse,
    CreateInteractionResponseMessage, CreateMessage, EditInteractionResponse, MessageFlags,
    ResolvedValue,
};
use serenity::model::prelude::ButtonStyle;
use serenity::prelude::SerenityError;

// needed for shutdown command
use tokio::sync::{mpsc::Sender, OnceCell};
use tokio::time::sleep;

pub static SHUTDOWN_SENDER: OnceCell<Sender<bool>> = OnceCell::const_new();

async fn send_simple_response_message<D>(
    ctx: &Context,
    command: &CommandInteraction,
    content: D,
    ephemeral: bool,
) -> Result<(), SerenityError>
where
    D: Into<String>,
{
    command
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content(content)
                    .ephemeral(ephemeral),
            ),
        )
        .await
}

async fn simple_defer(
    ctx: &Context,
    command: &CommandInteraction,
    ephemeral: bool,
) -> Result<(), SerenityError> {
    command
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Defer(
                CreateInteractionResponseMessage::new().ephemeral(ephemeral),
            ),
        )
        .await
}

async fn edit_deferred_message_simple<D>(
    ctx: &Context,
    command: &CommandInteraction,
    content: D,
) -> Result<(), SerenityError>
where
    D: Into<String>,
{
    command
        .edit_response(&ctx.http, EditInteractionResponse::new().content(content))
        .await?;
    Ok(())
}

pub fn create_commands() -> Vec<CreateCommand> {
    // DON'T FORGET to add your custom commands here!!
    vec![
        CreateCommand::new("help").description("Information on how to use the bot"),
        CreateCommand::new("ping").description("A ping command"),
        CreateCommand::new("shutdown").description("Shut down the bot"),
        CreateCommand::new("subscribe")
            .description("Receive notifications from a YouTube channel in this channel")
            .add_option(
                CreateCommandOption::new(
                    CommandOptionType::String,
                    "channel_url",
                    "Url of the YouTube channel",
                )
                .required(true),
            ),
    ]
}
// Any custom slash commands must be added both to create_commands ^^^ and to handle_command!!
pub async fn handle_command(
    ctx: Context,
    command: CommandInteraction,
) -> Result<(), SerenityError> {
    // Add any custom commands here
    match command.data.name.as_str() {
        "help" => help_command(ctx, command).await,
        "ping" => ping_command(ctx, command).await,
        "shutdown" => shutdown_command(ctx, command).await,
        "subscribe" => subscribe_command(ctx, command).await,
        _ => nyi_command(ctx, command).await,
    }
}

async fn nyi_command(ctx: Context, command: CommandInteraction) -> Result<(), SerenityError> {
    send_simple_response_message(
        &ctx,
        &command,
        "This command hasn't been implemented. Try /help",
        true,
    )
    .await
}

async fn help_command(ctx: Context, command: CommandInteraction) -> Result<(), SerenityError> {
    // This is very bare-bones, you will want to improve it most likely
    send_simple_response_message(
        &ctx,
        &command,
        "Currently available commands: `/ping`, `/shutdown`, `/help`.",
        true,
    )
    .await
    // for some reason you can't delete ephemeral interaction responses so I guess I'll just suffer
}

async fn ping_command(ctx: Context, command: CommandInteraction) -> Result<(), SerenityError> {
    let start_time = Instant::now();
    // Use awaiting the message as a delay to calculate the ping.
    // This gives very inconsistent results, but imo is probably closer to what you want than a heartbeat ping.
    simple_defer(&ctx, &command, true).await?;
    let mut duration = start_time.elapsed().as_millis().to_string();
    duration.push_str(" ms");
    command
        .edit_response(
            &ctx.http,
            EditInteractionResponse::new()
                .content(duration)
                .components(vec![CreateActionRow::Buttons(vec![make_button(
                    "refresh_ping",
                    ButtonStyle::Secondary,
                    Some('🔄'),
                    None,
                    false,
                )])]),
        )
        .await?;
    Ok(())
}

async fn shutdown_command(ctx: Context, command: CommandInteraction) -> Result<(), SerenityError> {
    // Set your admin user list in your config file
    let admins = ADMIN_USERS
        .get()
        .expect("Admin list somehow uninitialized??");
    if !admins.is_empty() && !admins.contains(&command.user.id) {
        send_simple_response_message(&ctx, &command, "You do not have permission.", true).await?;
        return Ok(());
    }
    println!(
        "Shutdown from user {} with Id {}",
        command.user.name, command.user.id
    );
    // no ? here, we don't want to return early if this fails
    _ = send_simple_response_message(&ctx, &command, "Shutting down...", true).await;
    // originally loosely based on https://stackoverflow.com/a/65456463
    // This error means that the shutdown channel is somehow not good, so we actually want to panic
    let sender = SHUTDOWN_SENDER
        .get()
        .expect("Shutdown command called before shutdown channel initialized??");
    // If this errors, the receiver could not receive the message anyways, so we want to panic
    sender
        .send(true)
        .await
        .expect("Shutdown message send error");
    println!("Passed shutdown message");
    // I'm pretty sure this is unnecessary but it makes me happier than not doing it
    ctx.shard.shutdown_clean();
    Ok(())
}

async fn subscribe_command(ctx: Context, command: CommandInteraction) -> Result<(), SerenityError> {
    simple_defer(&ctx, &command, true).await?;

    let channel_url = match &command.data.options()[0].value {
        ResolvedValue::String(s) => *s,
        v => {
            return edit_deferred_message_simple(
                &ctx,
                &command,
                format!("Invalid type for channel url parameter: {:?}", v),
            )
            .await
        }
    };

    let playlist_id = match get_upload_playlist_id(channel_url).await {
        Ok(v) => v,
        Err(PlaylistIdError::BadStatus(status)) => {
            return edit_deferred_message_simple(
                &ctx,
                &command,
                format!("HTTP request returned bad status code: {}", status),
            )
            .await
        }
        Err(PlaylistIdError::BodyParseError(e)) => {
            return edit_deferred_message_simple(
                &ctx,
                &command,
                format!(
                    "Could not find channel ID on webpage at webpage with address: \"{}\"",
                    e
                ),
            )
            .await
        }
        Err(PlaylistIdError::Hyper(e)) => {
            return edit_deferred_message_simple(&ctx, &command, format!("HTTP Error: {}", e)).await
        }
        Err(PlaylistIdError::UriParseError(_)) => {
            return edit_deferred_message_simple(
                &ctx,
                &command,
                format!(
                    "Invalid URL. Please make sure you typed it correctly.\nRecieved: {}",
                    channel_url
                ),
            )
            .await
        }
    };

    match add_channel(&playlist_id, command.channel_id).await {
        Ok(_) => {
            edit_deferred_message_simple(
                &ctx,
                &command,
                format!(
                    "Successfully subscribed channel {} to uploads playlist {}.",
                    command.channel_id.get(),
                    playlist_id
                ),
            )
            .await
        }
        Err(e) => {
            edit_deferred_message_simple(
                &ctx,
                &command,
                format!("Failed to add entry to database: {}", e),
            )
            .await
        }
    }
}

struct Workunit {
    video_id: String,
    channel_id: ChannelId,
}

pub async fn update_loop(sleep_seconds: u64, http: impl CacheHttp) {
    let duration = Duration::from_secs(sleep_seconds);
    loop {
        println!("Checking all playlists in DB.");
        let playlists = match get_playlists().await {
            Ok(v) => v,
            Err(e) => {
                println!("get_playlists in update_loop:\t{}", e);
                sleep(duration).await;
                continue;
            }
        };
        let playlists_len = playlists.len() as u32;
        println!("{} playlists", playlists_len);
        if playlists_len == 0 {
            sleep(duration).await;
            continue;
        }

        let mut workunits: Vec<Workunit> = vec![];
        for playlist_id in playlists.iter() {
            match get_uploads_from_playlist(&playlist_id).await {
                Err(UploadsError::MissingContent(mc)) => {
                    println!("get_uploads_from_playlist in update_loop:\t{:?}", mc)
                }
                Err(UploadsError::YouTube3(e)) => {
                    println!("get_uploads_from_playlist in update_loop:\t{}", e);
                }
                Ok(mut videos) => {
                    videos.reverse();
                    for video in videos {
                        match get_channels_to_send(&playlist_id, &video.published_at).await {
                            Err(e) => println!("get_channels_to_send in update_loop:\t{}", e),
                            Ok(channels) => {
                                for channel in channels {
                                    workunits.push(Workunit {
                                        video_id: video.id.clone(),
                                        channel_id: channel,
                                    })
                                }
                            }
                        }
                    }
                }
            };
        }

        let workunits_len = workunits.len();
        let duration = duration / workunits_len as u32;
        println!("{} workunits", workunits_len);
        for workunit in workunits {
            sleep(duration).await;
            match workunit
                .channel_id
                .send_message(
                    &http,
                    CreateMessage::new()
                        .content(format!("https://www.youtube.com/{}", workunit.video_id))
                        .flags(MessageFlags::empty()),
                )
                .await
            {
                Err(e) => println!("send_message in update_loop:\t{}", e),
                Ok(_) => (),
            };
        }
    }
}
