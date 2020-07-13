use crate::{
    config::Config,
    discord::discord_connection::ConnectionMeta,
    message_renderer::MessageRender,
    twilight_utils::ext::{ChannelExt, GuildChannelExt},
};
use anyhow::Result;
use std::{borrow::Cow, sync::Arc};
use tokio::sync::mpsc;
use tracing::*;
use twilight::{
    cache::InMemoryCache as Cache,
    model::{
        channel::{GuildChannel, Message},
        gateway::payload::MessageUpdate,
        id::{ChannelId, MessageId},
    },
};
use weechat::{
    buffer::{Buffer, BufferSettings},
    Weechat,
};

pub struct ChannelBuffer {
    renderer: MessageRender,
}

impl Drop for ChannelBuffer {
    fn drop(&mut self) {
        if !crate::ALIVE.alive() {
            return;
        }
        if let Ok(buffer) = self.renderer.buffer_handle.upgrade() {
            buffer.close();
        }
    }
}

impl ChannelBuffer {
    pub fn new(
        conn: &ConnectionMeta,
        config: &Config,
        channel: &GuildChannel,
        guild_name: &str,
        nick: &str,
        mut close_cb: impl FnMut(&Buffer) + 'static,
    ) -> Result<ChannelBuffer> {
        let clean_guild_name = crate::utils::clean_name(guild_name);
        let clean_channel_name = crate::utils::clean_name(&channel.name());
        let channel_id = channel.id();
        let guild_id = channel.guild_id();
        let conn_clone = conn.clone();
        let buffer_handle = Weechat::buffer_new(
            BufferSettings::new(&format!(
                "discord.{}.{}",
                clean_guild_name, clean_channel_name
            ))
            .input_callback(move |_: &Weechat, _: &Buffer, input: Cow<str>| {
                let http = conn_clone.http.clone();
                let cache = conn_clone.cache.clone();
                let input = input.to_string();
                conn_clone.rt.spawn(async move {
                    let input = crate::twilight_utils::content::create_mentions(
                        &cache,
                        Some(guild_id),
                        &input,
                    )
                    .await;
                    match http.create_message(channel_id).content(input) {
                        Ok(msg) => {
                            if let Err(e) = msg.await {
                                error!("Failed to send message: {:#?}", e);
                                Weechat::spawn_from_thread(async move {
                                    Weechat::print(&format!(
                                        "An error occured sending message: {}",
                                        e
                                    ))
                                });
                            };
                        },
                        Err(e) => {
                            error!("Failed to create message: {:#?}", e);
                            Weechat::spawn_from_thread(async {
                                Weechat::print("Message content's invalid")
                            })
                        },
                    }
                });
                Ok(())
            })
            .close_callback(move |_: &Weechat, buffer: &Buffer| {
                trace!(%channel_id, buffer.name=%buffer.name(), "Buffer close");
                close_cb(buffer);
                Ok(())
            }),
        )
        .map_err(|_| anyhow::anyhow!("Unable to create channel buffer"))?;

        let buffer = buffer_handle
            .upgrade()
            .map_err(|_| anyhow::anyhow!("Unable to upgrade buffer that was just created"))?;

        buffer.set_short_name(&format!("#{}", channel.name()));
        buffer.set_localvar("nick", nick);
        buffer.set_localvar("type", "channel");
        buffer.set_localvar("server", &clean_guild_name);
        buffer.set_localvar("channel", &clean_channel_name);
        if let Some(topic) = channel.topic() {
            buffer.set_title(&format!("#{} - {}", channel.name(), topic));
        } else {
            buffer.set_title(&format!("#{}", channel.name()));
        }

        Ok(ChannelBuffer {
            renderer: MessageRender::new(&conn, buffer_handle, config),
        })
    }
}

#[derive(Clone)]
pub struct DiscordChannel {
    channel_buffer: Arc<ChannelBuffer>,
    pub id: ChannelId,
    config: Config,
}

impl DiscordChannel {
    pub fn new(
        config: &Config,
        conn: &ConnectionMeta,
        channel: &GuildChannel,
        guild_name: &str,
        nick: &str,
        close_cb: impl FnMut(&Buffer) + 'static,
    ) -> Result<DiscordChannel> {
        let channel_buffer =
            ChannelBuffer::new(&conn.clone(), config, channel, guild_name, nick, close_cb)?;
        Ok(DiscordChannel {
            config: config.clone(),
            id: channel.id(),
            channel_buffer: Arc::new(channel_buffer),
        })
    }

    pub async fn load_history(&self, conn: &ConnectionMeta) -> Result<()> {
        let (mut tx, mut rx) = mpsc::channel(100);
        let conn_clone = conn.clone();
        {
            let id = self.id;
            let msg_count = self.config.message_fetch_count() as u64;

            conn.rt.spawn(async move {
                let messages: Vec<Message> = conn_clone
                    .http
                    .channel_messages(id)
                    .limit(msg_count)
                    .unwrap()
                    .await
                    .unwrap();
                tx.send(messages).await.unwrap();
            });
        }
        let messages = rx.recv().await.unwrap();

        self.channel_buffer
            .renderer
            .add_bulk_msgs(&conn.cache, &messages.into_iter().rev().collect::<Vec<_>>())
            .await;
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

    pub async fn update_message(&self, cache: &Cache, update: MessageUpdate) {
        self.channel_buffer.renderer.update_msg(cache, update).await;
    }

    pub async fn redraw(&self, cache: &Cache) {
        self.channel_buffer.renderer.redraw_buffer(cache).await;
    }
}
