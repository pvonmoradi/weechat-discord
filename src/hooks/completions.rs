use crate::{
    discord::discord_connection::DiscordConnection, twilight_utils::ext::ChannelExt, utils,
};
use std::borrow::Cow;
use weechat::{
    buffer::Buffer,
    hooks::{Completion, CompletionHook},
    Weechat,
};

pub struct Completions {
    _guild_completion_hook: CompletionHook,
    _channel_completion_hook: CompletionHook,
}

impl Completions {
    pub fn hook_all(weechat: &Weechat, connection: DiscordConnection) -> Completions {
        let connection_clone = connection.clone();
        let _guild_completion_hook = weechat
            .hook_completion(
                "discord_guild",
                "Completion for Discord servers",
                move |_: &Weechat, _: &Buffer, _: Cow<str>, completion: &Completion| {
                    // `list` should not have any completion items
                    if completion.arguments().splitn(3, ' ').nth(1) == Some("list") {
                        return Ok(());
                    }

                    if let Some(connection) = connection_clone.borrow().as_ref() {
                        let cache = connection.cache.clone();
                        let (tx, rx) = std::sync::mpsc::channel();
                        connection.rt.spawn(async move {
                            let guilds = cache
                                .guild_ids()
                                .await
                                .expect("InMemoryCache cannot fail")
                                .expect("guild_ids never fails");
                            for guild_id in guilds {
                                if let Some(guild) = cache
                                    .guild(guild_id)
                                    .await
                                    .expect("InMemoryCache cannot fail")
                                {
                                    tx.send(utils::clean_name(&guild.name))
                                        .expect("main thread panicked?");
                                }
                            }
                        });
                        while let Ok(cmp) = rx.recv() {
                            completion.add(&cmp);
                        }
                    }
                    Ok(())
                },
            )
            .expect("Unable to hook discord guild completion");

        let connection_clone = connection;
        let _channel_completion_hook = weechat
            .hook_completion(
                "discord_channel",
                "Completion for Discord channels",
                move |_: &Weechat, _: &Buffer, _: Cow<str>, completion: &Completion| {
                    // Get the previous argument which should be the guild name
                    let guild_name = match completion.arguments().splitn(4, ' ').nth(2) {
                        Some(guild_name) => guild_name.to_string(),
                        None => return Err(()),
                    };
                    let connection = connection_clone.borrow();
                    let connection = match connection.as_ref() {
                        Some(connection) => connection,
                        None => return Err(())
                    };

                    let cache = connection.cache.clone();

                    let (tx, rx) = std::sync::mpsc::channel();
                    connection.rt.spawn(async move {
                        match crate::twilight_utils::search_cached_striped_guild_name(
                            &cache,
                            &guild_name,
                        )
                            .await {
                            Some(guild) => {
                                if let Some(channels) = cache
                                    .channel_ids_in_guild(guild.id)
                                    .await
                                    .expect("InMemoryCache cannot fail")
                                {
                                    for channel_id in channels {
                                        match cache
                                            .guild_channel(channel_id)
                                            .await
                                            .expect("InMemoryCache cannot fail") {
                                            Some(channel) => {
                                                if !crate::twilight_utils::is_text_channel(&cache, channel.as_ref()).await { continue; }
                                                tx.send(utils::clean_name(&channel.name()))
                                                    .expect("main thread panicked?");
                                            }
                                            None => {
                                                tracing::trace!(id = %channel_id, "Unable to find channel in cache");
                                            }
                                        }
                                    }
                                }
                            }
                            None => {
                                tracing::trace!(name = %guild_name, "Unable to find guild");
                            }
                        }
                    });
                    while let Ok(cmp) = rx.recv() {
                        completion.add(&cmp);
                    }
                    Ok(())
                },
            )
            .expect("Unable to hook discord channel completion");

        Completions {
            _guild_completion_hook,
            _channel_completion_hook,
        }
    }
}
