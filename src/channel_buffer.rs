use crate::{
    config::Config,
    twilight_utils::{CachedGuildExt, GuildChannelExt},
};
use anyhow::Result;
use std::sync::mpsc::channel;
use twilight::{
    cache::InMemoryCache as Cache,
    http::Client as HttpClient,
    model::{
        channel::{ChannelType, GuildChannel, Message},
        guild::Permissions,
        id::ChannelId,
    },
};
use weechat::{
    buffer::{BufferHandle, BufferSettings},
    Weechat,
};

pub struct ChannelBuffer {
    buffer_handle: BufferHandle,
}

impl ChannelBuffer {
    pub fn new(channel: &GuildChannel, guild_name: &str) -> Result<ChannelBuffer> {
        let clean_guild_name = crate::utils::clean_name(guild_name);
        let clean_channel_name = crate::utils::clean_name(channel.name());
        let buffer_handle = Weechat::buffer_new(BufferSettings::new(&format!(
            "discord.{}.{}",
            clean_guild_name, clean_channel_name
        )))
        .map_err(|_| anyhow::anyhow!("Unable to create channel buffer"))?;

        let buffer = buffer_handle
            .upgrade()
            .map_err(|_| anyhow::anyhow!("Unable to upgrade buffer that was just created"))?;

        buffer.set_short_name(&format!("#{}", channel.name()));
        buffer.set_localvar("type", "channel");
        buffer.set_localvar("server", &clean_guild_name);
        if let Some(topic) = channel.topic() {
            buffer.set_title(&format!("#{} - {}", channel.name(), topic));
        } else {
            buffer.set_title(&format!("#{}", channel.name()));
        }

        Ok(ChannelBuffer { buffer_handle })
    }
}

pub struct DiscordChannel {
    channel_buffer: ChannelBuffer,
    id: ChannelId,
    config: Config,
}

impl DiscordChannel {
    pub fn new(
        config: &Config,
        channel: &GuildChannel,
        guild_name: &str,
    ) -> Result<DiscordChannel> {
        let channel_buffer = ChannelBuffer::new(channel, guild_name)?;
        Ok(DiscordChannel {
            config: config.clone(),
            id: channel.id(),
            channel_buffer,
        })
    }

    pub async fn load_history(
        &self,
        http: HttpClient,
        runtime: &tokio::runtime::Runtime,
    ) -> Result<()> {
        let (tx, rx) = channel();
        {
            let id = self.id;
            let msg_count = self.config.message_fetch_count() as u64;

            runtime.spawn(async move {
                let messages: Vec<Message> = http
                    .channel_messages(id)
                    .limit(msg_count)
                    .unwrap()
                    .await
                    .unwrap();
                tx.send(messages).unwrap();
            });
        }
        let messages = rx.recv().unwrap();

        let buffer = self
            .channel_buffer
            .buffer_handle
            .upgrade()
            .map_err(|_| anyhow::anyhow!("Unable to upgrade handle"))?;

        for msg in messages.iter().rev() {
            buffer.print_date_tags(
                chrono::DateTime::parse_from_rfc3339(&msg.timestamp)
                    .expect("Discord returned an invalid datetime")
                    .timestamp(),
                &[],
                &format!("{}\t{}", msg.author.name, msg.content),
            );
        }
        Ok(())
    }

    pub async fn is_text_channel(cache: &Cache, channel: &GuildChannel) -> bool {
        let current_user = match cache
            .current_user()
            .await
            .expect("InMemoryCache cannot fail")
        {
            Some(user) => user,
            None => return false,
        };

        let guild_id = match channel.guild_id() {
            Some(guild_id) => guild_id,
            None => return false,
        };
        let guild = match cache
            .guild(guild_id)
            .await
            .expect("InMemoryCache cannot fail")
        {
            Some(guild) => guild,
            None => return false,
        };

        if !guild
            .permissions_in(cache, channel.id(), current_user.id)
            .await
            .contains(Permissions::READ_MESSAGE_HISTORY)
        {
            return false;
        }

        match channel {
            GuildChannel::Category(c) => c.kind == ChannelType::GuildText,
            GuildChannel::Text(c) => c.kind == ChannelType::GuildText,
            GuildChannel::Voice(_) => false,
        }
    }
}
