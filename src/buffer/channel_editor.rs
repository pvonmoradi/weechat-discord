use crate::{
    config::Config,
    discord::discord_connection::ConnectionInner,
    twilight_utils::ext::GuildChannelExt,
    weechat2::{Style, StyledString},
};
use twilight_cache_inmemory::model::CachedGuild;
use twilight_model::channel::GuildChannel;
use weechat::{
    buffer::{Buffer, BufferBuilder},
    Weechat,
};

pub struct ChannelEditor(weechat::buffer::BufferHandle);

impl ChannelEditor {
    pub fn new(conn: &ConnectionInner, config: &Config) -> anyhow::Result<Self> {
        let buffer_name = "weecord.editor";
        let weechat = unsafe { Weechat::weechat() };

        if let Some(buffer) = weechat.buffer_search(crate::PLUGIN_NAME, &buffer_name) {
            buffer.close();
        };

        let handle = BufferBuilder::new(&buffer_name)
            .close_callback({
                move |_: &Weechat, _: &Buffer| {
                    tracing::trace!("Editor buffer close");
                    Ok(())
                }
            })
            .build()
            .map_err(|_| anyhow::anyhow!("Unable to create editor buffer"))?;

        let buffer = handle
            .upgrade()
            .map_err(|_| anyhow::anyhow!("Unable to create editor buffer"))?;

        buffer.set_free_content();

        buffer.set_short_name("Weecord channel editor");

        let mut y = 0;
        for guild_id in conn.cache.guild_ids().expect("Cache always returns some") {
            let guild = match conn.cache.guild(guild_id) {
                Some(guild) => guild,
                None => continue,
            };

            buffer.print_y(y, &Self::format_guild_line(config, &guild));
            y += 1;

            let guild_channels = match conn.cache.guild_channels(guild_id) {
                Some(channels) => channels,
                None => continue,
            };

            for channel_id in guild_channels {
                let channel = match conn.cache.guild_channel(channel_id) {
                    Some(channel) if channel.is_text_channel(&conn.cache) => channel,
                    _ => continue,
                };

                buffer.print_y(y, &Self::format_channel_line(config, &channel));
                y += 1;
            }
        }

        Ok(Self(handle))
    }

    fn format_channel_line(config: &Config, channel: &GuildChannel) -> String {
        let guild = channel
            .guild_id()
            .and_then(|guild_id| config.guilds().get(&guild_id).cloned());

        let autojoin = guild
            .as_ref()
            .map(|g| g.autojoin_channels().contains(&channel.id()))
            .unwrap_or(false);

        let watched = guild
            .map(|g| g.watched_channels().contains(&channel.id()))
            .unwrap_or(false);

        let mut channel_line = StyledString::new();
        channel_line
            .push_str(&format!("  {:20} ", channel.name()))
            .push_styled_str(Style::Reset, "[")
            .push_styled_str(Style::color("green"), "autojoin")
            .push_styled_str(Style::Bold, "=");
        if autojoin {
            channel_line.push_styled_str(Style::color("cyan"), "yes");
        } else {
            channel_line.push_styled_str(Style::color("yellow"), "no ");
        }
        channel_line
            .push_styled_str(Style::Reset, "] [")
            .push_styled_str(Style::color("green"), "watched")
            .push_styled_str(Style::Bold, "=");
        if watched {
            channel_line.push_styled_str(Style::color("cyan"), "yes");
        } else {
            channel_line.push_styled_str(Style::color("yellow"), "no ");
        }
        channel_line.push_styled_str(Style::Reset, "]");
        channel_line.build()
    }

    fn format_guild_line(config: &Config, guild: &CachedGuild) -> String {
        let autoconnect = config
            .guilds()
            .get(&guild.id)
            .map(|g| g.autoconnect())
            .unwrap_or(false);

        let mut guild_line = StyledString::new();
        guild_line
            .push_str(&format!("{:22} ", &guild.name))
            .push_styled_str(Style::Reset, "[")
            .push_styled_str(Style::color("green"), "autoconnect")
            .push_styled_str(Style::Bold, "=");
        if autoconnect {
            guild_line.push_styled_str(Style::color("cyan"), "yes");
        } else {
            guild_line.push_styled_str(Style::color("yellow"), "no ");
        }
        guild_line.push_styled_str(Style::Reset, "]");
        guild_line.build()
    }
}
