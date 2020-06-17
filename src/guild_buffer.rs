use crate::{
    channel_buffer::DiscordChannel, config::Config, discord::discord_connection::DiscordConnection,
    twilight_utils::ext::GuildChannelExt, Guilds,
};
use anyhow::Result;
use std::{
    cell::{RefCell, RefMut},
    collections::HashMap,
    rc::{Rc, Weak},
};
use tracing::*;
use twilight::{
    cache::InMemoryCache as Cache,
    http::Client as HttpClient,
    model::id::{ChannelId, GuildId},
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

    pub async fn connect(
        &self,
        cache: &Cache,
        http: &HttpClient,
        rt: &tokio::runtime::Runtime,
        connection: DiscordConnection,
        guilds: Guilds,
    ) -> Result<()> {
        if let Some(guild) = cache.guild(self.id).await? {
            let mut inner = self.inner.borrow_mut();

            if inner.guild_buffer.is_some() {
                return Ok(());
            }

            inner
                .guild_buffer
                .replace(GuildBuffer::new(guilds, self.id, &guild.name)?);

            let current_user = cache
                .current_user()
                .await
                .expect("InMemoryCache cannot fail")
                .expect("We have a connection, there must be a user");

            let member = cache
                .member(guild.id, current_user.id)
                .await
                .expect("InMemoryCache cannot fail");

            let nick = if let Some(member) = member {
                crate::utils::color::colorize_discord_member(cache, member.as_ref()).await
            } else {
                current_user.name.clone()
            };

            for channel_id in inner.autojoin.clone() {
                if let Some(channel) = cache.guild_channel(channel_id).await? {
                    if crate::twilight_utils::is_text_channel(&cache, &channel).await {
                        trace!(channel = %channel.name(), "Creating channel buffer");
                        if let Ok(buf) = DiscordChannel::new(
                            &self.config,
                            connection.clone(),
                            self.clone(),
                            &channel,
                            &guild.name,
                            &nick,
                        ) {
                            if let Err(e) = buf.load_history(cache, http.clone(), &rt).await {
                                warn!(
                                    error = ?e,
                                    channel = %channel.name(),
                                    "Failed to load channel history",
                                )
                            }
                            inner.buffers.insert(channel_id, buf);
                        }
                    }
                }
            }
        } else {
            warn!(guild_id = self.id.0, "Unable to find cached guild");
        }
        Ok(())
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
    }
}
