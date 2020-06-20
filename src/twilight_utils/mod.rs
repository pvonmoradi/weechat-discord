use crate::{
    twilight_utils::ext::{CachedGuildExt, GuildChannelExt},
    utils,
};
use std::sync::Arc;
use twilight::{
    cache::{twilight_cache_inmemory::model::CachedGuild, InMemoryCache as Cache},
    model::{
        channel::{ChannelType, GuildChannel},
        guild::Permissions,
        id::GuildId,
    },
};

mod color;
pub mod content;
pub mod ext;
pub mod mention;
use crate::twilight_utils::ext::ChannelExt;
pub use color::*;
pub use mention::*;

pub async fn search_cached_striped_guild_name(
    cache: &Cache,
    target: &str,
) -> Option<Arc<CachedGuild>> {
    crate::twilight_utils::search_striped_guild_name(
        cache,
        cache
            .guild_ids()
            .await
            .expect("InMemoryCache cannot fail")
            .expect("guild_ids never fails"),
        target,
    )
    .await
}

pub async fn search_striped_guild_name(
    cache: &Cache,
    guilds: impl IntoIterator<Item = GuildId>,
    target: &str,
) -> Option<Arc<CachedGuild>> {
    for guild_id in guilds {
        if let Some(guild) = cache
            .guild(guild_id)
            .await
            .expect("InMemoryCache cannot fail")
        {
            if utils::clean_name(&guild.name) == utils::clean_name(target) {
                return Some(guild);
            }
        } else {
            tracing::warn!("{:?} not found in cache", guild_id);
        }
    }
    None
}

pub async fn search_cached_stripped_guild_channel_name(
    cache: &Cache,
    guild_id: GuildId,
    target: &str,
) -> Option<Arc<GuildChannel>> {
    let channels = cache
        .channel_ids_in_guild(guild_id)
        .await
        .expect("InMemoryCache cannot fail")
        .expect("guild_ids never fails");
    for channel_id in channels {
        if let Some(channel) = cache
            .guild_channel(channel_id)
            .await
            .expect("InMemoryCache cannot fail")
        {
            if !crate::twilight_utils::is_text_channel(cache, &channel).await {
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

pub async fn is_text_channel(cache: &Cache, channel: &GuildChannel) -> bool {
    let current_user = match cache
        .current_user()
        .await
        .expect("InMemoryCache cannot fail")
    {
        Some(user) => user,
        None => return false,
    };

    let guild = match cache
        .guild(channel.guild_id())
        .await
        .expect("InMemoryCache cannot fail")
    {
        Some(guild) => guild,
        None => return false,
    };

    if !guild
        .permissions_in(cache, channel.id(), current_user.id)
        .await
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

pub async fn current_user_nick(guild: &CachedGuild, cache: &Cache) -> String {
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
        crate::utils::color::colorize_discord_member(cache, member.as_ref(), false).await
    } else {
        current_user.name.clone()
    };
    nick
}
