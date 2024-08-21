use crate::generate_components::make_button;
use crate::ADMIN_USERS;

use std::time::Instant;

use serenity::all::{
    CommandInteraction, Context, CreateActionRow, CreateCommand, CreateInteractionResponse,
    CreateInteractionResponseMessage, EditInteractionResponse,
};
use serenity::model::prelude::ButtonStyle;
use serenity::prelude::SerenityError;

// needed for shutdown command
use tokio::sync::{mpsc::Sender, OnceCell};

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

pub fn create_commands() -> Vec<CreateCommand> {
    // DON'T FORGET to add your custom commands here!!
    vec![
        CreateCommand::new("help").description("Information on how to use the bot"),
        CreateCommand::new("ping").description("A ping command"),
        CreateCommand::new("shutdown").description("Shut down the bot"),
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
    command
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Defer(
                CreateInteractionResponseMessage::new().ephemeral(true),
            ),
        )
        .await?;
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
