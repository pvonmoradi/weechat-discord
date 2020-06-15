use crate::{config::Config, twilight_utils::ext::GuildChannelExt};
use anyhow::Result;
use std::sync::mpsc::channel;
use twilight::{
    http::Client as HttpClient,
    model::{
        channel::{GuildChannel, Message},
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
}
