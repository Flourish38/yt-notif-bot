# rust-discord-bot-template

This is a fleshed out template to start a discord bot using [serenity](https://github.com/serenity-rs/serenity).

### How to use it

Put your token in a file called `config.(ini|json|yaml|toml|ron|json5)` with the key "token".
You can also specify admin users in an array with the key "admins". By default, this is only used for the shutdown command.

For example, a file `config.toml` would look like:
```toml
token = "TOKEN_GOES_HERE"
admins = [ 123456789876543210 ]
```

A default configuration file is provided at `src/config.toml`.
In order to use it, simply move it out of `src/`. It should be in the same directory as `Cargo.toml`.
This will mean that the config file is untracked by default,
which is important so you ***don't commit your discord token to a public repository.***

Alternatively, you can instead provide your token via the environment variable `DISCORD_TOKEN`.
This will override the value provided in the config file, if any.

Once you decide on a file format for your config file, you can disable the ones you aren't using in `Cargo.toml`
according to [config-rs features](https://github.com/mehcode/config-rs#feature-flags).
Don't forget to use `default-features = false` if you do this.

From there, add more commands in `src/commands.rs`, and implement any necessary components in `src/components.rs`.
You ***shouldn't*** need to modify `src/main.rs` at all; since the config is accessible as a static variable,
you can just add more keys to the config file without having to specify anything in `src/main.rs`.
The only exception is if you need to use a different type of interaction than commands and components,
which you could simply add to the match statement on line 39 of `src/main.rs`.


Template made by [Flourish38](https://github.com/Flourish38).
