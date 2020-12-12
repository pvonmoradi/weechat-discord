use crate::{
    buffer::channel::Channel,
    config::{Config, GuildConfig},
    discord::discord_connection::ConnectionInner,
    instance::Instance,
    refcell::RefCell,
};
use std::{collections::HashMap, rc::Rc};
use twilight_cache_inmemory::model::CachedGuild as TwilightGuild;
use twilight_model::{
    channel::GuildChannel as TwilightChannel,
    id::{ChannelId, GuildId},
};
use weechat::{
    buffer::{Buffer, BufferBuilder, BufferHandle},
    Weechat,
};

pub struct GuildBuffer(BufferHandle);

impl GuildBuffer {
    pub fn new(name: &str, id: GuildId, instance: Instance) -> anyhow::Result<Self> {
        let clean_guild_name = crate::utils::clean_name(&name);
        // TODO: Check for existing buffer before creating one
        let handle = BufferBuilder::new(&format!("discord.{}", clean_guild_name))
            .close_callback({
                let name = name.to_string();
                move |_: &Weechat, _: &Buffer| {
                    tracing::trace!(buffer.id=%id, buffer.name=%name, "Buffer close");
                    if let Ok(mut instance) = instance.try_borrow_guilds_mut() {
                        if let Some(x) = instance.remove(&id) {
                            x.inner.borrow_mut().closed = true;
                        }
                    }
                    Ok(())
                }
            })
            .build()
            .map_err(|_| anyhow::anyhow!("Unable to create guild buffer"))?;

        let buffer = handle
            .upgrade()
            .map_err(|_| anyhow::anyhow!("Unable to create guild buffer"))?;

        buffer.set_short_name(&name);
        buffer.set_localvar("type", "server");
        buffer.set_localvar("server", &clean_guild_name);
        buffer.set_localvar("guild_id", &id.0.to_string());

        Ok(GuildBuffer(handle))
    }
}

pub struct GuildInner {
    conn: ConnectionInner,
    buffer: Option<GuildBuffer>,
    channels: HashMap<ChannelId, Channel>,
    closed: bool,
}

impl Drop for GuildInner {
    fn drop(&mut self) {
        // This feels ugly, but without it, closing a buffer causes this struct to drop, which in turn
        // causes a segfault (for some reason)
        if self.closed {
            return;
        }
        if let Some(buffer) = self.buffer.as_ref() {
            if let Ok(buffer) = buffer.0.upgrade() {
                buffer.close();
            }
        }
    }
}

impl GuildInner {
    pub fn new(conn: ConnectionInner) -> Self {
        Self {
            conn,
            buffer: None,
            channels: HashMap::new(),
            closed: false,
        }
    }
}

#[derive(Clone)]
pub struct Guild {
    pub(crate) id: GuildId,
    inner: Rc<RefCell<GuildInner>>,
    pub guild_config: GuildConfig,
    pub config: Config,
}

impl Guild {
    pub fn debug_counts(&self) -> (usize, usize) {
        (Rc::strong_count(&self.inner), Rc::weak_count(&self.inner))
    }

    pub fn new(
        id: GuildId,
        conn: ConnectionInner,
        guild_config: GuildConfig,
        config: &Config,
    ) -> Self {
        let inner = Rc::new(RefCell::new(GuildInner::new(conn)));
        Guild {
            id,
            inner,
            guild_config,
            config: config.clone(),
        }
    }

    pub fn connect(&self, instance: Instance) -> anyhow::Result<()> {
        let mut inner = self.inner.borrow_mut();
        if let Some(guild) = inner.conn.cache.guild(self.id) {
            inner
                .buffer
                .replace(GuildBuffer::new(&guild.name, guild.id, instance.clone())?);

            let conn = inner.conn.clone();
            for auto_channel_id in self.guild_config.autojoin_channels() {
                if let Some(channel) = conn.cache.guild_channel(auto_channel_id) {
                    if crate::twilight_utils::is_text_channel(&conn.cache, &channel) {
                        tracing::info!("Joining channel: #{}", channel.name());

                        self._join_channel(&channel, &guild, &mut inner)?;
                    }
                }
            }

            Ok(())
        } else {
            tracing::warn!(guild_id=%self.id, "guild not cached");
            Err(anyhow::anyhow!("Guild: {} is not in the cache", self.id))
        }
    }

    fn _join_channel(
        &self,
        channel: &TwilightChannel,
        guild: &TwilightGuild,
        inner: &mut GuildInner,
    ) -> anyhow::Result<()> {
        let weak_inner = Rc::downgrade(&self.inner);
        let channel_id = channel.id();
        let channel = crate::buffer::channel::Channel::guild(
            &channel,
            &guild,
            &inner.conn,
            &self.config,
            move |_| {
                if let Some(inner) = weak_inner.upgrade() {
                    if let Ok(mut inner) = inner.try_borrow_mut() {
                        if let Some(channel) = inner.channels.remove(&channel_id) {
                            channel.set_closed();
                        }
                    }
                }
            },
        )?;

        inner.channels.insert(channel_id, channel);

        Ok(())
    }

    pub fn join_channel(
        &self,
        channel: &TwilightChannel,
        guild: &TwilightGuild,
    ) -> anyhow::Result<()> {
        self._join_channel(channel, guild, &mut self.inner.borrow_mut())
    }

    pub fn channels(&self) -> HashMap<ChannelId, Channel> {
        self.inner.borrow().channels.clone()
    }
}
