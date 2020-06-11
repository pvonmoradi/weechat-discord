use crate::{channel_buffer::DiscordChannel, config::Config, twilight_utils::GuildChannelExt};
use anyhow::Result;
use std::{
    cell::RefCell,
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
    buffer::{BufferHandle, BufferSettings},
    config::{BooleanOptionSettings, ConfigSection, StringOption, StringOptionSettings},
    Weechat,
};

pub struct GuildBuffer {
    _buffer_handle: BufferHandle,
}

impl GuildBuffer {
    pub fn new(guild_name: &str) -> Result<GuildBuffer> {
        let clean_guild_name = crate::utils::clean_name(guild_name);
        let buffer_handle = Weechat::buffer_new(BufferSettings::new(&format!(
            "discord.{}",
            clean_guild_name
        )))
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

pub struct DiscordGuild {
    id: GuildId,
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
                value.split(',').all(|ch| ch.parse::<u64>().is_ok())
            })
            .set_change_callback(move |_, option| {
                let inner = inner_clone.upgrade().expect("Config has outlived guild");

                inner.borrow_mut().autojoin = option
                    .value()
                    .split(',')
                    .map(|ch| ch.parse().map(ChannelId))
                    .flatten()
                    .collect();
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
    ) -> Result<()> {
        if let Some(guild) = cache.guild(self.id).await? {
            let mut inner = self.inner.borrow_mut();

            if inner.guild_buffer.is_some() {
                return Ok(());
            }

            inner.guild_buffer.replace(GuildBuffer::new(&guild.name)?);

            for channel_id in inner.autojoin.clone() {
                if let Some(channel) = cache.guild_channel(channel_id).await? {
                    if DiscordChannel::is_text_channel(&cache, &channel).await {
                        trace!(channel = %channel.name(), "Creating channel buffer");
                        if let Ok(buf) = DiscordChannel::new(&self.config, &channel, &guild.name) {
                            if let Err(e) = buf.load_history(http.clone(), &rt).await {
                                warn!(
                                    error = ?e,
                                    channel = %channel.name(),
                                    "Failed to load channel history",
                                )
                            }
                            inner.buffers.insert(channel_id.clone(), buf);
                        }
                    }
                }
            }
        } else {
            warn!(guild_id = self.id.0, "Unable to find cached guild");
        }
        Ok(())
    }

    pub fn autoconnect(&self) -> bool {
        self.inner.borrow().autoconnect
    }
}
