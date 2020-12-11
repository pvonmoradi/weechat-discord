use crate::{
    config::Config,
    discord::discord_connection::ConnectionInner,
    message_renderer::MessageRender,
    nicklist::Nicklist,
    refcell::RefCell,
    twilight_utils::ext::{ChannelExt, GuildChannelExt},
};
use parsing::LineEdit;
use std::{borrow::Cow, rc::Rc, sync::Arc};
use tokio::sync::mpsc;
use twilight_cache_inmemory::{
    model::{CachedGuild as TwilightGuild, CachedMember},
    InMemoryCache as Cache,
};
use twilight_http::Client as HttpClient;
use twilight_model::{
    channel::{
        GuildChannel as TwilightGuildChannel, Message, PrivateChannel as TwilightPrivateChannel,
    },
    gateway::payload::MessageUpdate,
    id::{ChannelId, GuildId, MessageId, UserId},
    user::User,
};
use weechat::{
    buffer::{Buffer, BufferBuilder},
    Weechat,
};

pub struct GuildChannelBuffer {
    renderer: MessageRender,
    nicklist: Nicklist,
}

impl GuildChannelBuffer {
    pub fn new(
        name: &str,
        nick: &str,
        guild_name: &str,
        id: ChannelId,
        guild_id: GuildId,
        conn: &ConnectionInner,
        config: &Config,
        mut close_cb: impl FnMut(&Buffer) + 'static,
    ) -> anyhow::Result<Self> {
        let clean_guild_name = crate::utils::clean_name(&guild_name);
        let clean_channel_name = crate::utils::clean_name(&name);
        // TODO: Check for existing buffer before creating one
        let handle = BufferBuilder::new(&format!(
            "discord.{}.{}",
            clean_guild_name, clean_channel_name
        ))
        .input_callback({
            let conn = conn.clone();
            move |_: &Weechat, _: &Buffer, input: Cow<str>| {
                send_message(id, Some(guild_id), &conn, &input);
                Ok(())
            }
        })
        .close_callback({
            let name = name.to_string();
            move |_: &Weechat, buffer: &Buffer| {
                tracing::trace!(buffer.id=%id, buffer.name=%name, "Buffer close");
                close_cb(buffer);
                Ok(())
            }
        })
        .build()
        .map_err(|_| anyhow::anyhow!("Unable to create channel buffer"))?;

        let buffer = handle
            .upgrade()
            .map_err(|_| anyhow::anyhow!("Unable to create guild buffer"))?;

        buffer.set_localvar("nick", nick);

        buffer.set_short_name(&format!("#{}", name));
        buffer.set_localvar("type", "channel");
        buffer.set_localvar("server", &clean_guild_name);
        buffer.set_localvar("channel", &clean_channel_name);
        buffer.set_localvar("guild_id", &guild_id.0.to_string());
        buffer.set_localvar("channel_id", &id.0.to_string());

        buffer.enable_nicklist();

        let handle = Rc::new(handle);
        Ok(Self {
            renderer: MessageRender::new(conn, Rc::clone(&handle), config),
            nicklist: Nicklist::new(conn, handle),
        })
    }

    pub async fn add_members(&self, members: &[Arc<CachedMember>]) {
        self.nicklist.add_members(members).await;
    }
}

pub struct PrivateChannelBuffer {
    renderer: MessageRender,
    nicklist: Nicklist,
}

impl PrivateChannelBuffer {
    pub fn new(
        channel: &TwilightPrivateChannel,
        conn: &ConnectionInner,
        config: &Config,
        mut close_cb: impl FnMut(&Buffer) + 'static,
    ) -> anyhow::Result<Self> {
        let id = channel.id;

        let short_name = PrivateChannelBuffer::short_name(&channel.recipients);
        let buffer_id = PrivateChannelBuffer::buffer_id(&channel.recipients);

        let handle = BufferBuilder::new(&buffer_id)
            .input_callback({
                let conn = conn.clone();
                move |_: &Weechat, _: &Buffer, input: Cow<str>| {
                    send_message(id, None, &conn, &input);
                    Ok(())
                }
            })
            .close_callback({
                let short_name = short_name.to_string();
                move |_: &Weechat, buffer: &Buffer| {
                    tracing::trace!(buffer.id=%id, buffer.name=%short_name, "Buffer close");
                    close_cb(buffer);
                    Ok(())
                }
            })
            .build()
            .map_err(|_| anyhow::anyhow!("Unable to create channel buffer"))?;

        let buffer = handle
            .upgrade()
            .map_err(|_| anyhow::anyhow!("Unable to create guild buffer"))?;

        buffer.set_localvar("nick", &PrivateChannelBuffer::nick(&conn.cache));

        let full_name = channel.name();

        buffer.set_short_name(&short_name);
        buffer.set_full_name(&full_name);
        buffer.set_title(&full_name);
        // This causes the buffer to be indented, what are the implications for not setting it?
        // buffer.set_localvar("type", "private");
        buffer.set_localvar("channel_id", &id.0.to_string());

        let handle = Rc::new(handle);
        Ok(Self {
            renderer: MessageRender::new(&conn, Rc::clone(&handle), config),
            nicklist: Nicklist::new(conn, handle),
        })
    }

    fn nick(cache: &Cache) -> String {
        format!(
            "@{}",
            cache
                .current_user()
                .map(|u| u.name.clone())
                .expect("No current user?")
        )
    }

    fn short_name(recipients: &[User]) -> String {
        format!(
            "DM with {}",
            recipients
                .iter()
                .map(|u| u.name.clone())
                .collect::<Vec<_>>()
                .join(", ")
        )
    }

    fn buffer_id(recipients: &[User]) -> String {
        format!(
            "discord.dm.{}",
            &recipients
                .iter()
                .map(|u| crate::utils::clean_name(&u.name))
                .collect::<Vec<_>>()
                .join(".")
        )
    }

    pub async fn add_members(&self, members: &[Arc<CachedMember>]) {
        self.nicklist.add_members(members).await;
    }
}

enum ChannelBufferVariants {
    GuildChannel(GuildChannelBuffer),
    PrivateChannel(PrivateChannelBuffer),
}

impl ChannelBufferVariants {
    fn renderer(&self) -> &MessageRender {
        use ChannelBufferVariants::*;
        let renderer = match self {
            GuildChannel(buffer) => &buffer.renderer,
            PrivateChannel(buffer) => &buffer.renderer,
        };
        renderer
    }

    pub fn close(&self) {
        if let Ok(buffer) = self.renderer().buffer_handle.upgrade() {
            buffer.close();
        }
    }

    pub fn add_bulk_msgs(&self, cache: &Cache, msgs: &[Message]) {
        self.renderer().add_bulk_msgs(cache, msgs)
    }

    pub fn add_msg(&self, cache: &Cache, msg: &Message, notify: bool) {
        self.renderer().add_msg(cache, msg, notify)
    }

    pub fn remove_msg(&self, cache: &Cache, id: MessageId) {
        self.renderer().remove_msg(cache, id)
    }

    pub fn update_msg(&self, cache: &Cache, update: MessageUpdate) {
        self.renderer().update_msg(cache, update)
    }

    pub fn redraw_buffer(&self, cache: &Cache, ignore_users: &[UserId]) {
        self.renderer().redraw_buffer(cache, ignore_users)
    }

    pub async fn add_members(&self, members: &[Arc<CachedMember>]) {
        use ChannelBufferVariants::*;
        match self {
            GuildChannel(buffer) => buffer.add_members(members).await,
            PrivateChannel(buffer) => buffer.add_members(members).await,
        }
    }
}

struct ChannelInner {
    conn: ConnectionInner,
    buffer: ChannelBufferVariants,
    closed: bool,
}

impl Drop for ChannelInner {
    fn drop(&mut self) {
        // This feels ugly, but without it, closing a buffer causes this struct to drop, which in turn
        // causes a segfault (for some reason)
        if self.closed {
            return;
        }

        self.buffer.close();
    }
}

impl ChannelInner {
    pub fn new(conn: ConnectionInner, buffer: ChannelBufferVariants) -> Self {
        Self {
            conn,
            buffer,
            closed: false,
        }
    }
}

#[derive(Clone)]
pub struct Channel {
    pub(crate) id: ChannelId,
    guild_id: Option<GuildId>,
    inner: Rc<RefCell<ChannelInner>>,
    config: Config,
}

impl Channel {
    pub fn debug_counts(&self) -> (usize, usize) {
        (Rc::strong_count(&self.inner), Rc::weak_count(&self.inner))
    }

    pub fn guild(
        channel: &TwilightGuildChannel,
        guild: &TwilightGuild,
        conn: &ConnectionInner,
        config: &Config,
        close_cb: impl FnMut(&Buffer) + 'static,
    ) -> anyhow::Result<Self> {
        let nick = format!(
            "@{}",
            crate::twilight_utils::current_user_nick(&guild, &conn.cache)
        );
        let channel_buffer = GuildChannelBuffer::new(
            channel.name(),
            &nick,
            &guild.name,
            channel.id(),
            guild.id,
            conn,
            config,
            close_cb,
        )?;
        let inner = Rc::new(RefCell::new(ChannelInner::new(
            conn.clone(),
            ChannelBufferVariants::GuildChannel(channel_buffer),
        )));
        Ok(Channel {
            id: channel.id(),
            guild_id: Some(guild.id),
            inner,
            config: config.clone(),
        })
    }

    pub fn private(
        channel: &TwilightPrivateChannel,
        conn: &ConnectionInner,
        config: &Config,
        close_cb: impl FnMut(&Buffer) + 'static,
    ) -> anyhow::Result<Self> {
        let channel_buffer = PrivateChannelBuffer::new(&channel, conn, config, close_cb)?;
        let inner = Rc::new(RefCell::new(ChannelInner::new(
            conn.clone(),
            ChannelBufferVariants::PrivateChannel(channel_buffer),
        )));
        Ok(Channel {
            id: channel.id,
            guild_id: None,
            inner,
            config: config.clone(),
        })
    }

    pub async fn load_history(&self) -> anyhow::Result<()> {
        let (mut tx, mut rx) = mpsc::channel(100);
        let conn = &self.inner.borrow().conn;
        let conn_clone = conn.clone();
        {
            let id = self.id;
            let msg_count = self.config.message_fetch_count() as u64;

            conn.rt.spawn(async move {
                let mut messages: Vec<_> = conn_clone
                    .http
                    .channel_messages(id)
                    .limit(msg_count)
                    .unwrap()
                    .await
                    .unwrap();

                // This is a bit of a hack because the returned messages have no guild id, even if
                // they are from a guild channel
                if let Some(guild_channel) = conn_clone.cache.guild_channel(id) {
                    for msg in messages.iter_mut() {
                        msg.guild_id = guild_channel.guild_id()
                    }
                }
                tx.send(messages).await.unwrap();
            });
        }
        let messages = rx.recv().await.unwrap();

        self.inner
            .borrow()
            .buffer
            .add_bulk_msgs(&conn.cache, &messages.into_iter().rev().collect::<Vec<_>>());
        Ok(())
    }

    pub async fn load_users(&self) -> anyhow::Result<()> {
        let conn = &self.inner.borrow().conn;
        if let Some(channel) = conn.cache.guild_channel(self.id) {
            if let Ok(members) = channel.members(&conn.cache) {
                self.inner.borrow().buffer.add_members(&members).await;
                Ok(())
            } else {
                tracing::error!(guild.id=?self.guild_id, channel.id=%self.id, "unable to load members for nicklist");
                Err(anyhow::anyhow!("unable to load members for nicklist"))
            }
        } else {
            tracing::warn!(guild.id=?self.guild_id, channel.id=%self.id, "unable to find channel in cache");
            Err(anyhow::anyhow!("unable to load members for nicklist"))
        }
    }

    pub fn add_message(&self, cache: &Cache, msg: &Message, notify: bool) {
        self.inner.borrow().buffer.add_msg(cache, msg, notify);
    }

    pub fn remove_message(&self, cache: &Cache, msg_id: MessageId) {
        self.inner.borrow().buffer.remove_msg(cache, msg_id);
    }

    pub fn update_message(&self, cache: &Cache, update: MessageUpdate) {
        self.inner.borrow().buffer.update_msg(cache, update);
    }

    pub fn redraw(&self, cache: &Cache, ignore_users: &[UserId]) {
        self.inner
            .borrow()
            .buffer
            .redraw_buffer(cache, ignore_users);
    }

    pub fn set_closed(&self) {
        let _ = self
            .inner
            .try_borrow_mut()
            .map(|mut inner| inner.closed = true);
    }
}

fn send_message(id: ChannelId, guild_id: Option<GuildId>, conn: &ConnectionInner, input: &str) {
    let cache = conn.cache.clone();
    let input = crate::twilight_utils::content::create_mentions(&cache, guild_id, &input);
    let http = conn.http.clone();
    conn.rt.spawn(async move {
        let current_user_id = cache.current_user().expect("No current user?").id;
        match parsing::parse_line_edit(&input) {
            Some(LineEdit::Sub {
                line,
                old,
                new,
                options,
            }) => {
                if let Some(msg) = get_users_nth_message(current_user_id, &http, id, line).await {
                    let orig = msg.content.clone();

                    let e = http.update_message(id, msg.id);
                    let future = if options.map(|o| o.contains('g')).unwrap_or_default() {
                        e.content(orig.replace(old, new))
                    } else {
                        e.content(orig.replacen(old, new, 1))
                    }
                    .expect("new content is always Some");

                    if let Err(e) = future.await {
                        tracing::error!("Unable to update message: {}", e);
                    } else {
                        tracing::trace!("Successfully updated message {}", msg.id);
                    };
                } else {
                    tracing::warn!("Unable to find message n {}", line);
                    Weechat::spawn_from_thread(async move {
                        Weechat::print(&format!("discord: unable to locate message n = {}", line))
                    })
                };
            },
            Some(LineEdit::Delete { line }) => {
                if let Some(msg) = get_users_nth_message(current_user_id, &http, id, line).await {
                    if let Err(e) = http.delete_message(id, msg.id).await {
                        tracing::error!("Unable to delete message: {}", e);
                    } else {
                        tracing::trace!("Successfully deleted message {}", msg.id);
                    };
                } else {
                    tracing::warn!("Unable to find message n {}", line);
                    Weechat::spawn_from_thread(async move {
                        Weechat::print(&format!("discord: unable to locate message n = {}", line))
                    })
                };
            },
            None => match http.create_message(id).content(input) {
                Ok(msg) => {
                    if let Err(e) = msg.await {
                        tracing::error!("Failed to send message: {:#?}", e);
                        Weechat::spawn_from_thread(async move {
                            Weechat::print(&format!(
                                "discord: an error occurred sending message: {}",
                                e
                            ))
                        });
                    };
                },
                Err(e) => {
                    tracing::error!("Failed to create message: {:#?}", e);
                    Weechat::spawn_from_thread(async {
                        Weechat::print("discord: message content is invalid")
                    })
                },
            },
        };
    });
}

async fn get_users_nth_message(
    user: UserId,
    http: &HttpClient,
    channel_id: ChannelId,
    n: usize,
) -> Option<Message> {
    if n > 100 {
        tracing::warn!("n exceeds message fetch limit, retrieval will be unsuccessful");
    }
    http.channel_messages(channel_id)
        .await
        .ok()
        .and_then(|msgs| {
            msgs.into_iter()
                .filter(|msg| msg.author.id == user)
                .nth(n - 1)
        })
}
