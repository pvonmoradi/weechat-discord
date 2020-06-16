use crate::{
    config::Config, message_renderer::MessageRender, twilight_utils::ext::GuildChannelExt,
};
use anyhow::Result;
use std::sync::{mpsc::channel, Arc};
use twilight::{
    cache::InMemoryCache as Cache,
    http::Client as HttpClient,
    model::{
        channel::{GuildChannel, Message},
        id::{ChannelId, MessageId},
    },
};
use weechat::{buffer::BufferSettings, Weechat};

pub struct ChannelBuffer {
    renderer: MessageRender,
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

        Ok(ChannelBuffer {
            renderer: MessageRender::new(buffer_handle),
        })
    }
}

#[derive(Clone)]
pub struct DiscordChannel {
    channel_buffer: Arc<ChannelBuffer>,
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
            channel_buffer: Arc::new(channel_buffer),
        })
    }

    pub async fn load_history(
        &self,
        cache: &Cache,
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

        for msg in messages.iter().rev() {
            self.channel_buffer
                .renderer
                .add_msg(cache, msg, false)
                .await;
        }
        Ok(())
    }

    pub async fn add_message(&self, cache: &Cache, msg: &Message, notify: bool) {
        self.channel_buffer
            .renderer
            .add_msg(cache, msg, notify)
            .await;
    }

    pub async fn remove_message(&self, cache: &Cache, msg_id: MessageId) {
        self.channel_buffer.renderer.remove_msg(cache, msg_id).await;
    }
}
