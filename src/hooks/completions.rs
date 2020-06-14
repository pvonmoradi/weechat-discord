use crate::{discord::discord_connection::DiscordConnection, utils};
use std::borrow::Cow;
use weechat::{
    buffer::Buffer,
    hooks::{Completion, CompletionHook},
    Weechat,
};

pub struct Completions {
    _guild_completion_hook: CompletionHook,
}

impl Completions {
    pub fn hook_all(weechat: &Weechat, connection: DiscordConnection) -> Completions {
        let _guild_completion_hook = weechat
            .hook_completion(
                "discord_guild",
                "Completion for Discord guilds",
                move |_: &Weechat, _: &Buffer, _: Cow<str>, completion: &Completion| {
                    // `list` should not have any completion items
                    if completion.arguments().splitn(3, " ").nth(1) == Some("list") {
                        return Ok(());
                    }

                    if let Some(connection) = &*connection.borrow() {
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

        Completions {
            _guild_completion_hook,
        }
    }
}
