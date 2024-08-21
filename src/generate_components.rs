use serenity::all::CreateButton;
use serenity::model::prelude::ButtonStyle;
use serenity::model::prelude::ReactionType;

// The generic option makes you really think twice about making a button with no emoji,
// since it makes you type, for instance, `None::<char>`.
// This is my way of subtly encouraging you to use emojis for your buttons,
// they are an excellent accessibility feature for non-native speakers.
pub fn make_button<D: Into<String>, E: Into<ReactionType>>(
    custom_id: D,
    style: ButtonStyle,
    emoji: Option<E>,
    label: Option<&str>,
    disabled: bool,
) -> CreateButton {
    let mut button = CreateButton::new(custom_id).style(style).disabled(disabled);
    if let Some(emoji) = emoji {
        button = button.emoji(emoji.into());
    }
    if let Some(label) = label {
        button = button.label(label);
    }
    button
}
