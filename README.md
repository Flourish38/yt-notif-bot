# yt-notif-bot

This is a discord bot that sends a message whenever a YouTube channel that you specify uploads.

### How to run it

Put your token in a file called `config.(ini|json|yaml|toml|ron|json5)` with the key "token".
You will need to include a [YouTube Data API key](https://developers.google.com/youtube/v3/getting-started#before-you-start) with the key "key".
You can also specify admin users in an array with the key "admins". Only users in the admins list can shut down your bot with /shutdown.

For example, a file `config.toml` would look like:
```toml
token = "TOKEN_GOES_HERE"
key = "KEY_GOES_HERE"
admins = [ 123456789876543210 ]
```

A default configuration file is provided at `src/config.toml`.
In order to use it, simply move it out of `src/`. It should be in the same directory as `Cargo.toml`.
This will mean that the config file is untracked by default,
which is important so you ***don't commit your discord token or API key to a public repository.***

Alternatively, you can instead provide your token/key via the environment variables `DISCORD_TOKEN`/`YOUTUBE_KEY`.
This will override the value provided in the config file, if any.

Once you decide on a file format for your config file, you may disable the ones you aren't using in `Cargo.toml`
according to [config-rs features](https://github.com/mehcode/config-rs#feature-flags).
Don't forget to use `default-features = false` if you do this.

Then, from the same directory that contains `Cargo.toml`, simply run `cargo run --release`!
If you don't have cargo installed, you can [get it here](https://doc.rust-lang.org/cargo/getting-started/installation.html).
It will probably take a few minutes to compile, but certainly less than 15 minutes.

## How to use it

Simply type `/subscribe CHANNEL_URL` to receive a discord message in that channel whenever that YouTube channel uploads a new video.
It will automatically catch up if it ever misses a video due to being offline, so don't worry about missing any notifications!

## Words of Warning

Currently, there is no way to unsubscribe from a channel. The only ways to do this would be to:
- Delete the 3 sqlite.db files and start over (easiest, but you have to start from scratch)
- Open the sqlite db using another program and remove the unwanted rows (moderate difficulty, simplest)
    - You can identify the right `playlist_id` to delete by going to someone's channel page,
    clicking "...more" on their description, scrolling down to "Share channel", and selecting
    "Copy channel ID". It will be of the form `UCxxxx...xxx`, and the corresponding `playlist_id`
    will be of the form `UUxxxx...xxx`.
- Modify the code directly (hardest, and I'll probably be doing it soon anyways, so not recommended.)
    - If you know what you're doing, though, I wouldn't complain about a pull request!

This bot is configured by default to attempt to use all 10,000 daily quota units from the YouTube Data API.
If you give other projects the same key, someone is going to get rate limited.