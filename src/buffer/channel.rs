use crate::{
    config::Config,
    discord::discord_connection::ConnectionInner,
    instance::Instance,
    message_renderer::MessageRender,
    nicklist::Nicklist,
    refcell::RefCell,
    twilight_utils::ext::{ChannelExt, GuildChannelExt},
};
use parsing::{Emoji, LineEdit};
use std::{borrow::Cow, rc::Rc, sync::Arc};
use tokio::sync::mpsc;
use twilight_cache_inmemory::{
    model::{CachedGuild as TwilightGuild, CachedMember},
    InMemoryCache as Cache,
};
use twilight_http::request::channel::reaction::RequestReactionType;
use twilight_model::{
    channel::{
        message::MessageReaction, GuildChannel as TwilightGuildChannel, Message,
        PrivateChannel as TwilightPrivateChannel, Reaction,
    },
    gateway::payload::MessageUpdate,
    id::{ChannelId, EmojiId, GuildId, MessageId, UserId},
    user::User,
};
use weechat::{
    buffer::{Buffer, BufferBuilder},
    Weechat,
};

struct ChannelBuffer {
    pub renderer: MessageRender,
    pub nicklist: Nicklist,
}

impl ChannelBuffer {
    #[allow(clippy::too_many_arguments)]
    pub fn guild(
        name: &str,
        nick: &str,
        guild_name: &str,
        id: ChannelId,
        guild_id: GuildId,
        conn: &ConnectionInner,
        config: &Config,
        instance: &Instance,
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
            let instance = instance.clone();
            move |_: &Weechat, _: &Buffer, input: Cow<str>| {
                if let Some(channel) = instance.search_buffer(Some(guild_id), id) {
                    send_message(&channel, &conn, &input);
                }
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

    pub fn private(
        channel: &TwilightPrivateChannel,
        conn: &ConnectionInner,
        config: &Config,
        instance: &Instance,
        mut close_cb: impl FnMut(&Buffer) + 'static,
    ) -> anyhow::Result<Self> {
        let id = channel.id;

        let short_name = Self::short_name(&channel.recipients);
        let buffer_id = Self::buffer_id(&channel.recipients);

        let handle = BufferBuilder::new(&buffer_id)
            .input_callback({
                let conn = conn.clone();
                let instance = instance.clone();
                move |_: &Weechat, _: &Buffer, input: Cow<str>| {
                    if let Some(channel) = instance.search_buffer(None, id) {
                        send_message(&channel, &conn, &input);
                    }
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

        buffer.set_localvar("nick", &Self::nick(&conn.cache));

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

    pub fn close(&self) {
        if let Ok(buffer) = self.renderer.buffer_handle.upgrade() {
            buffer.close();
        }
    }

    pub fn add_bulk_msgs(&self, cache: &Cache, msgs: &[Message]) {
        self.renderer.add_bulk_msgs(cache, msgs)
    }

    pub fn add_msg(&self, cache: &Cache, msg: &Message, notify: bool) {
        self.renderer.add_msg(cache, msg, notify)
    }

    pub fn add_reaction(&self, cache: &Cache, reaction: Reaction) {
        self.renderer.update_message(reaction.message_id, |msg| {
            // Copied from twilight
            if let Some(msg_reaction) = msg.reactions.iter_mut().find(|r| r.emoji == reaction.emoji)
            {
                if !msg_reaction.me {
                    if let Some(current_user) = cache.current_user() {
                        if current_user.id == reaction.user_id {
                            msg_reaction.me = true;
                        }
                    }
                }

                msg_reaction.count += 1;
            } else {
                let me = cache
                    .current_user()
                    .map(|user| user.id == reaction.user_id)
                    .unwrap_or_default();

                msg.reactions.push(MessageReaction {
                    count: 1,
                    emoji: reaction.emoji.clone(),
                    me,
                });
            }
        });
        self.renderer.redraw_buffer(cache, &[]);
    }

    pub fn remove_reaction(&self, cache: &Cache, reaction: Reaction) {
        self.renderer.update_message(reaction.message_id, |msg| {
            // TODO: Use Vec::drain_filter when it stabilizes
            if let Some((i, reaction)) = msg
                .reactions
                .iter_mut()
                .enumerate()
                .find(|(_, r)| r.emoji == reaction.emoji)
            {
                if reaction.count == 1 {
                    msg.reactions.remove(i);
                } else {
                    reaction.count -= 1
                }
            }
        });
        self.renderer.redraw_buffer(cache, &[]);
    }

    pub fn remove_msg(&self, cache: &Cache, id: MessageId) {
        self.renderer.remove_msg(cache, id)
    }

    pub fn update_msg(&self, cache: &Cache, update: MessageUpdate) {
        self.renderer.apply_message_update(cache, update)
    }

    pub fn redraw_buffer(&self, cache: &Cache, ignore_users: &[UserId]) {
        self.renderer.redraw_buffer(cache, ignore_users)
    }

    pub fn add_members(&self, members: &[Arc<CachedMember>]) {
        self.nicklist.add_members(members);
    }
}

struct ChannelInner {
    conn: ConnectionInner,
    buffer: ChannelBuffer,
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
    pub fn new(conn: ConnectionInner, buffer: ChannelBuffer) -> Self {
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
        instance: &Instance,
        close_cb: impl FnMut(&Buffer) + 'static,
    ) -> anyhow::Result<Self> {
        let nick = format!(
            "@{}",
            crate::twilight_utils::current_user_nick(&guild, &conn.cache)
        );
        let channel_buffer = ChannelBuffer::guild(
            channel.name(),
            &nick,
            &guild.name,
            channel.id(),
            guild.id,
            conn,
            config,
            instance,
            close_cb,
        )?;
        let inner = Rc::new(RefCell::new(ChannelInner::new(
            conn.clone(),
            channel_buffer,
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
        instance: &Instance,
        close_cb: impl FnMut(&Buffer) + 'static,
    ) -> anyhow::Result<Self> {
        let channel_buffer = ChannelBuffer::private(&channel, conn, config, instance, close_cb)?;
        let inner = Rc::new(RefCell::new(ChannelInner::new(
            conn.clone(),
            channel_buffer,
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
        let conn = self.inner.borrow().conn.clone();
        {
            let id = self.id;
            let msg_count = self.config.message_fetch_count() as u64;

            let conn_clone = conn.clone();
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

        let inner = self.inner.borrow();
        inner.buffer.add_bulk_msgs(
            &inner.conn.cache,
            &messages.into_iter().rev().collect::<Vec<_>>(),
        );
        Ok(())
    }

    pub fn load_users(&self) -> anyhow::Result<()> {
        let conn = self.inner.borrow().conn.clone();
        if let Some(channel) = conn.cache.guild_channel(self.id) {
            if let Ok(members) = channel.members(&conn.cache) {
                // TODO: Fix this, currently there doesn't seem to be much we can do about it
                #[allow(clippy::await_holding_refcell_ref)]
                self.inner.borrow().buffer.add_members(&members);
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

    pub fn add_reaction(&self, cache: &Cache, reaction: Reaction) {
        self.inner.borrow().buffer.add_reaction(cache, reaction);
    }

    pub fn remove_reaction(&self, cache: &Cache, reaction: Reaction) {
        self.inner.borrow().buffer.remove_reaction(cache, reaction);
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

fn send_message(channel: &Channel, conn: &ConnectionInner, input: &str) {
    let channel = channel.clone();
    let id = channel.id;
    let guild_id = channel.guild_id;
    let conn = conn.clone();
    let cache = conn.cache.clone();
    let http = conn.http.clone();
    let input = crate::twilight_utils::content::create_mentions(&cache, guild_id, &input);
    match parsing::LineEdit::parse(&input) {
        Some(LineEdit::Sub {
            line,
            old,
            new,
            options,
        }) => {
            if let Some(msg) = channel
                .inner
                .borrow()
                .buffer
                .renderer
                .get_nth_message(line - 1)
            {
                let orig = msg.content.clone();
                let old = old.to_string();
                let new = new.to_string();
                let options = options.map(ToString::to_string);

                conn.rt.spawn(async move {
                    let e = http.update_message(id, msg.id);
                    let future = if options.map(|o| o.contains('g')).unwrap_or_default() {
                        e.content(orig.replace(&old, &new))
                    } else {
                        e.content(orig.replacen(&old, &new, 1))
                    }
                    .expect("new content is always Some");

                    if let Err(e) = future.await {
                        tracing::error!("Unable to update message: {}", e);
                    } else {
                        tracing::trace!("Successfully updated message {}", msg.id);
                    };
                });
            } else {
                tracing::warn!("Unable to find message n {}", line);
                Weechat::print(&format!("discord: unable to locate message n = {}", line));
            };
        },
        Some(LineEdit::Delete { line }) => {
            if let Some(msg) = channel
                .inner
                .borrow()
                .buffer
                .renderer
                .get_nth_message(line - 1)
            {
                conn.rt.spawn(async move {
                    if let Err(e) = http.delete_message(id, msg.id).await {
                        tracing::error!("Unable to delete message: {}", e);
                    } else {
                        tracing::trace!("Successfully deleted message {}", msg.id);
                    };
                });
            } else {
                tracing::warn!("Unable to find message n {}", line);
                Weechat::print(&format!("discord: unable to locate message n = {}", line))
            };
        },
        None => {
            if let Some(reaction) = parsing::Reaction::parse(&input) {
                let add = reaction.add;
                let reaction_type = match reaction.emoji {
                    Emoji::Shortcode(name) => {
                        if let Some(emoji) = discord_emoji::lookup(name) {
                            RequestReactionType::Unicode {
                                name: emoji.to_string(),
                            }
                        } else {
                            return;
                        }
                    },
                    Emoji::Custom(name, id) => RequestReactionType::Custom {
                        id: EmojiId(id),
                        name: Some(name.to_string()),
                    },
                    Emoji::Unicode(name) => RequestReactionType::Unicode {
                        name: name.to_string(),
                    },
                };

                if let Some(msg) = channel
                    .inner
                    .borrow()
                    .buffer
                    .renderer
                    .get_nth_message(reaction.line - 1)
                {
                    conn.rt.spawn(async move {
                        if add {
                            if let Err(e) = http.create_reaction(id, msg.id, reaction_type).await {
                                tracing::error!("Failed to add reaction: {:#?}", e);
                                Weechat::spawn_from_thread(async move {
                                    Weechat::print(&format!(
                                        "discord: an error occurred adding reaction: {}",
                                        e
                                    ))
                                });
                            }
                        } else if let Err(e) = http
                            .delete_current_user_reaction(id, msg.id, reaction_type)
                            .await
                        {
                            tracing::error!("Failed to remove reaction: {:#?}", e);
                            Weechat::spawn_from_thread(async move {
                                Weechat::print(&format!(
                                    "discord: an error occurred removing reaction: {}",
                                    e
                                ))
                            });
                        }
                    });
                }

                return;
            };
            let input = input.clone();
            conn.rt.spawn(async move {
                match http.create_message(id).content(input) {
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
                        Weechat::spawn_from_thread(async move {
                            Weechat::print("discord: message content is invalid")
                        });
                    },
                }
            });
        },
    };
}
