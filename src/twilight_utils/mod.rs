use crate::{twilight_utils::ext::CachedGuildExt, utils};
use std::sync::Arc;
use twilight_cache_inmemory::{model::CachedGuild, InMemoryCache as Cache};
use twilight_model::{
    channel::{ChannelType, GuildChannel},
    guild::Permissions,
    id::GuildId,
};

mod color;
pub mod content;
mod dynamic_channel;
pub mod ext;
pub mod mention;

pub use color::*;
pub use dynamic_channel::*;
pub use mention::*;

pub fn search_cached_striped_guild_name(cache: &Cache, target: &str) -> Option<Arc<CachedGuild>> {
    crate::twilight_utils::search_striped_guild_name(
        cache,
        cache.guild_ids().expect("guild_ids never fails"),
        target,
    )
}

pub fn search_striped_guild_name(
    cache: &Cache,
    guilds: impl IntoIterator<Item = GuildId>,
    target: &str,
) -> Option<Arc<CachedGuild>> {
    for guild_id in guilds {
        if let Some(guild) = cache.guild(guild_id) {
            if utils::clean_name(&guild.name) == utils::clean_name(target) {
                return Some(guild);
            }
        } else {
            tracing::warn!("{:?} not found in cache", guild_id);
        }
    }
    None
}

pub fn search_cached_stripped_guild_channel_name(
    cache: &Cache,
    guild_id: GuildId,
    target: &str,
) -> Option<Arc<GuildChannel>> {
    let channels = cache
        .channel_ids_in_guild(guild_id)
        .expect("guild_ids never fails");
    for channel_id in channels {
        if let Some(channel) = cache.guild_channel(channel_id) {
            if !crate::twilight_utils::is_text_channel(cache, &channel) {
                continue;
            }
            if utils::clean_name(&channel.name()) == utils::clean_name(target) {
                return Some(channel);
            }
        } else {
            tracing::warn!("{:?} not found in cache", channel_id);
        }
    }
    None
}

pub fn is_text_channel(cache: &Cache, channel: &GuildChannel) -> bool {
    let current_user = match cache.current_user() {
        Some(user) => user,
        None => return false,
    };

    let guild = match cache.guild(channel.guild_id().expect("GuildChannel must have guild id")) {
        Some(guild) => guild,
        None => return false,
    };

    if !guild
        .permissions_in(cache, channel.id(), current_user.id)
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

pub fn current_user_nick(guild: &CachedGuild, cache: &Cache) -> String {
    let current_user = cache
        .current_user()
        .expect("We have a connection, there must be a user");

    let member = cache.member(guild.id, current_user.id);

    let nick = if let Some(member) = member {
        crate::utils::color::colorize_discord_member(cache, member.as_ref(), false)
    } else {
        current_user.name.clone()
    };
    nick
}
