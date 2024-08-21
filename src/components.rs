use std::time::Instant;

use serenity::all::{
    ComponentInteraction, Context, CreateInteractionResponse, CreateInteractionResponseMessage,
    EditInteractionResponse,
};
use serenity::prelude::SerenityError;

pub async fn handle_component(
    ctx: Context,
    component: ComponentInteraction,
) -> Result<(), SerenityError> {
    // Add any custom components here
    match component.data.custom_id.as_str() {
        "refresh_ping" => ping_refresh_component(ctx, component).await,
        _ => nyi_component(ctx, component).await,
    }
}

async fn nyi_component(ctx: Context, component: ComponentInteraction) -> Result<(), SerenityError> {
    component
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content("Component interaction not yet implemented.")
                    .ephemeral(true),
            ),
        )
        .await
}

async fn ping_refresh_component(
    ctx: Context,
    component: ComponentInteraction,
) -> Result<(), SerenityError> {
    let start_time = Instant::now();
    // Use awaiting the defer as a delay to calculate the ping.
    // This gives very inconsistent results, but imo is probably closer to what you want than a heartbeat ping.
    component.defer(&ctx.http).await?;
    let mut duration = start_time.elapsed().as_millis().to_string();
    duration.push_str(" ms");
    // This does not remove the refresh component from the original message.
    component
        .edit_response(&ctx.http, EditInteractionResponse::new().content(duration))
        .await?;
    Ok(())
}
