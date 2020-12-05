use crate::{
    buffer::ext::BufferExt, discord::discord_connection::DiscordConnection, instance::Instance,
};
use weechat::{
    hooks::{SignalData, SignalHook},
    ReturnCode, Weechat,
};

pub struct Signals {
    _buffer_switch_hook: SignalHook,
}

impl Signals {
    pub fn hook_all(connection: DiscordConnection, instance: Instance) -> Signals {
        let _buffer_switch_hook = SignalHook::new(
            "buffer_switch",
            move |_: &Weechat, _: &str, data: Option<SignalData>| {
                if let Some(SignalData::Buffer(buffer)) = data {
                    if buffer.history_loaded() {
                        return ReturnCode::Ok;
                    }

                    let guild_id = buffer.guild_id();

                    let channel_id = match buffer.channel_id() {
                        Some(channel_id) => channel_id,
                        None => {
                            return ReturnCode::Ok;
                        },
                    };

                    if let Some(channel) = instance.search_buffer(guild_id, channel_id) {
                        buffer.set_history_loaded();
                        let connection = connection.clone();
                        Weechat::spawn(async move {
                            tracing::trace!(?guild_id, ?channel_id, "Sending channel subscription");
                            if let Some(guild_id) = guild_id {
                                connection
                                    .send_guild_subscription(guild_id, channel_id)
                                    .await;
                            }
                        });
                        Weechat::spawn({
                            let channel = channel.clone();
                            async move {
                                tracing::trace!(?guild_id, ?channel_id, "Loading history");
                                if let Err(e) = channel.load_history().await {
                                    tracing::error!(
                                        ?guild_id,
                                        ?channel_id,
                                        "Error loading channel history: {}",
                                        e
                                    );
                                }
                            }
                        });
                        Weechat::spawn({
                            async move {
                                if let Err(e) = channel.load_users().await {
                                    tracing::error!(
                                        ?guild_id,
                                        ?channel_id,
                                        "Error loading channel member list: {}",
                                        e
                                    );
                                }
                            }
                        });
                    }
                }
                ReturnCode::Ok
            },
        )
        .expect("Unable to hook buffer_switch signal");

        Signals {
            _buffer_switch_hook,
        }
    }
}
