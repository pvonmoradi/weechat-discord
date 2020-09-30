use crate::instance::Instance;
use twilight_model::id::{ChannelId, GuildId};
use weechat::{
    hooks::{SignalData, SignalHook},
    ReturnCode, Weechat,
};

pub struct Signals {
    _buffer_switch_hook: SignalHook,
}

impl Signals {
    pub fn hook_all(instance: Instance) -> Signals {
        let _buffer_switch_hook = SignalHook::new(
            "buffer_switch",
            move |_: &Weechat, _: &str, data: Option<SignalData>| {
                if let Some(SignalData::Buffer(buffer)) = data {
                    if buffer
                        .get_localvar("loaded_history")
                        .unwrap_or_else(|| "false".into())
                        == "true"
                    {
                        return ReturnCode::Ok;
                    }

                    let guild_id = buffer
                        .get_localvar("guild_id")
                        .and_then(|id| id.parse().ok())
                        .map(GuildId);

                    let channel_id = match buffer
                        .get_localvar("channel_id")
                        .and_then(|id| id.parse().ok())
                        .map(ChannelId)
                    {
                        Some(channel_id) => channel_id,
                        None => {
                            return ReturnCode::Ok;
                        },
                    };

                    if let Some(channel) = instance.search_buffer(guild_id, channel_id) {
                        buffer.set_localvar("loaded_history", "true");
                        Weechat::spawn(async move {
                            tracing::trace!(?guild_id, ?channel_id, "Loading history");
                            if let Err(e) = channel.load_history().await {
                                tracing::error!("Error loading channel history: {}", e);
                            }

                            if let Err(e) = channel.load_users().await {
                                tracing::error!("Error loading channel member list: {}", e);
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
