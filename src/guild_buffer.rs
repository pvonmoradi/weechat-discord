use crate::{
    channel_buffer::DiscordChannel,
    config::Config,
    discord::discord_connection::ConnectionMeta,
    refcell::{RefCell, RefMut},
    twilight_utils::ext::ChannelExt,
    Guilds,
};
use anyhow::Result;
use std::{
    collections::HashMap,
    rc::{Rc, Weak},
};
use tracing::*;
use twilight::{
    cache::twilight_cache_inmemory::model::CachedGuild,
    model::{
        channel::GuildChannel,
        id::{ChannelId, GuildId},
    },
};
use weechat::{
    buffer::{Buffer, BufferHandle, BufferSettings},
    config::{
        BaseConfigOption, BooleanOptionSettings, ConfigSection, StringOption, StringOptionSettings,
    },
    Weechat,
};

pub struct GuildBuffer {
    _buffer_handle: BufferHandle,
}

impl GuildBuffer {
    pub fn new(guilds: Guilds, guild_id: GuildId, guild_name: &str) -> Result<GuildBuffer> {
        let clean_guild_name = crate::utils::clean_name(guild_name);
        let buffer_handle = Weechat::buffer_new(
            BufferSettings::new(&format!("discord.{}", clean_guild_name))
                .close_callback(move |_: &Weechat, buffer: &Buffer| {
                    tracing::trace!(buffer.id=%guild_id, buffer.name=%buffer.name(), "Buffer close");
                    guilds.borrow_mut().remove(&guild_id);
                    Ok(())
                }),
        )
            .map_err(|_| anyhow::anyhow!("Unable to create guild buffer"))?;

        let buffer = buffer_handle
            .upgrade()
            .map_err(|_| anyhow::anyhow!("Unable to upgrade buffer that was just created"))?;

        buffer.set_short_name(guild_name);
        buffer.set_localvar("type", "server");
        buffer.set_localvar("server", &clean_guild_name);

        Ok(GuildBuffer {
            _buffer_handle: buffer_handle,
        })
    }
}

pub struct InnerGuild {
    guild_buffer: Option<GuildBuffer>,
    buffers: HashMap<ChannelId, DiscordChannel>,
    autoconnect: bool,
    autojoin: Vec<ChannelId>,
}

impl InnerGuild {
    pub fn new() -> InnerGuild {
        InnerGuild {
            buffers: Default::default(),
            guild_buffer: None,
            autoconnect: false,
            autojoin: Vec::new(),
        }
    }
}

#[derive(Clone)]
pub struct DiscordGuild {
    pub id: GuildId,
    inner: Rc<RefCell<InnerGuild>>,
    config: Config,
}

impl DiscordGuild {
    pub fn new(config: &Config, id: GuildId, guild_section: &mut ConfigSection) -> DiscordGuild {
        let inner = Rc::new(RefCell::new(InnerGuild::new()));

        let weak_inner = Rc::downgrade(&inner);

        let inner_clone = Weak::clone(&weak_inner);
        let autoconnect = BooleanOptionSettings::new(format!("{}.autoconnect", id.0))
            .set_change_callback(move |_, option| {
                let inner = inner_clone.upgrade().expect("Config has outlived guild");

                inner.borrow_mut().autoconnect = option.value();
            });
        guild_section
            .new_boolean_option(autoconnect)
            .expect("Unable to create autoconnect option");

        let inner_clone = Weak::clone(&weak_inner);
        let autojoin_channels = StringOptionSettings::new(format!("{}.autojoin", id.0))
            .set_check_callback(|_: &Weechat, _: &StringOption, value| {
                if value.is_empty() {
                    true
                } else {
                    value.split(',').all(|ch| ch.parse::<u64>().is_ok())
                }
            })
            .set_change_callback(move |_, option| {
                let inner = inner_clone.upgrade().expect("Config has outlived guild");

                let mut channels: Vec<_> = option
                    .value()
                    .split(',')
                    .map(|ch| ch.parse().map(ChannelId))
                    .flatten()
                    .collect();

                channels.sort();
                channels.dedup();

                option.set(
                    &channels
                        .iter()
                        .map(|c| c.0.to_string())
                        .collect::<Vec<_>>()
                        .join(","),
                    false,
                );

                inner.borrow_mut().autojoin = channels;
            });
        guild_section
            .new_string_option(autojoin_channels)
            .expect("Unable to create autojoin channels option");

        DiscordGuild {
            config: config.clone(),
            id,
            inner,
        }
    }

    pub async fn connect(&self, conn: &ConnectionMeta, guilds: Guilds) -> Result<()> {
        if let Some(guild) = conn.cache.guild(self.id).await? {
            {
                let mut inner = self.inner.borrow_mut();

                if inner.guild_buffer.is_some() {
                    return Ok(());
                }

                inner
                    .guild_buffer
                    .replace(GuildBuffer::new(guilds, self.id, &guild.name)?);
            }

            let channels = self.inner.borrow().autojoin.clone();
            for channel_id in channels {
                if let Some(channel) = conn.cache.guild_channel(channel_id).await? {
                    if crate::twilight_utils::is_text_channel(&conn.cache, &channel).await {
                        trace!(channel = %channel.name(), "Creating channel buffer");
                        self.join_channel(conn, &guild, &channel).await;
                    }
                }
            }
        } else {
            warn!(guild_id = self.id.0, "Unable to find cached guild");
        }
        Ok(())
    }

    pub async fn join_channel(
        &self,
        conn: &ConnectionMeta,
        guild: &CachedGuild,
        channel: &GuildChannel,
    ) -> Option<DiscordChannel> {
        let nick = crate::twilight_utils::current_user_nick(&guild, &conn.cache).await;

        if let Ok(buf) = DiscordChannel::new(
            &self.config,
            conn,
            self.clone(),
            &channel,
            &guild.name,
            &nick,
        ) {
            if let Err(e) = buf.load_history(conn).await {
                warn!(
                    error = ?e,
                    channel = %channel.name(),
                    "Failed to load channel history",
                )
            }
            self.inner
                .borrow_mut()
                .buffers
                .insert(channel.id(), buf.clone());
            Some(buf)
        } else {
            None
        }
    }

    pub fn autojoin(&self) -> Vec<ChannelId> {
        self.inner.borrow().autojoin.clone()
    }

    pub fn autojoin_mut(&self) -> RefMut<Vec<ChannelId>> {
        RefMut::map(self.inner.borrow_mut(), |i| &mut i.autojoin)
    }

    pub fn autoconnect(&self) -> bool {
        self.inner.borrow().autoconnect
    }

    pub fn set_autoconnect(&self, autoconnect: bool) {
        self.inner.borrow_mut().autoconnect = autoconnect;
    }

    pub fn channel_buffers(&self) -> HashMap<ChannelId, DiscordChannel> {
        self.inner.borrow().buffers.clone()
    }

    pub fn channel_buffers_mut(&self) -> RefMut<HashMap<ChannelId, DiscordChannel>> {
        RefMut::map(self.inner.borrow_mut(), |i| &mut i.buffers)
    }

    pub fn write_config(&self) {
        let config = self.config.config.borrow();
        let section = config
            .search_section("server")
            .expect("Unable to get server section");

        let autojoin = section
            .search_option(&format!("{}.autojoin", self.id))
            .expect("autojoin option does not exist");
        autojoin.set(
            &self
                .autojoin()
                .iter()
                .map(|c| c.0.to_string())
                .collect::<Vec<_>>()
                .join(","),
            true,
        );

        let autoconnect = section
            .search_option(&format!("{}.autoconnect", self.id))
            .expect("autoconnect option does not exist");
        autoconnect.set(if self.autoconnect() { "on" } else { "off" }, false);
    }
}
