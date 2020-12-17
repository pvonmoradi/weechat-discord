use crate::twilight_utils::ext::{GuildChannelExt, UserExt};
use twilight_cache_inmemory::{InMemoryCache as Cache, InMemoryCache};
use twilight_model::{
    channel::{ChannelType, Group, GuildChannel, PrivateChannel},
    id::ChannelId,
};
use twilight_permission_calculator::prelude::Permissions;

pub trait ChannelExt {
    fn name(&self) -> String;
    fn id(&self) -> ChannelId;
    fn kind(&self) -> ChannelType;
    fn can_send(&self, cache: &Cache) -> Option<bool>;
}

impl ChannelExt for GuildChannel {
    fn name(&self) -> String {
        match self {
            GuildChannel::Category(c) => c.name.clone(),
            GuildChannel::Text(c) => c.name.clone(),
            GuildChannel::Voice(c) => c.name.clone(),
        }
    }

    fn id(&self) -> ChannelId {
        match self {
            GuildChannel::Category(c) => c.id,
            GuildChannel::Text(c) => c.id,
            GuildChannel::Voice(c) => c.id,
        }
    }

    fn kind(&self) -> ChannelType {
        match self {
            GuildChannel::Category(c) => c.kind,
            GuildChannel::Text(c) => c.kind,
            GuildChannel::Voice(c) => c.kind,
        }
    }

    fn can_send(&self, cache: &Cache) -> Option<bool> {
        self.has_permission_in_channel(cache, Permissions::SEND_MESSAGES)
    }
}

impl ChannelExt for PrivateChannel {
    fn name(&self) -> String {
        format!(
            "DM with {}",
            self.recipients
                .iter()
                .map(UserExt::tag)
                .collect::<Vec<_>>()
                .join(", ")
        )
    }

    fn id(&self) -> ChannelId {
        self.id
    }

    fn kind(&self) -> ChannelType {
        self.kind
    }

    fn can_send(&self, cache: &Cache) -> Option<bool> {
        let current_user = cache.current_user()?;
        Some(self.recipients.iter().any(|rec| rec.id == current_user.id))
    }
}

impl ChannelExt for Group {
    fn name(&self) -> String {
        format!(
            "DM with {}",
            self.recipients
                .iter()
                .map(UserExt::tag)
                .collect::<Vec<_>>()
                .join(", ")
        )
    }

    fn id(&self) -> ChannelId {
        self.id
    }

    fn kind(&self) -> ChannelType {
        self.kind
    }

    fn can_send(&self, cache: &InMemoryCache) -> Option<bool> {
        let current_user = cache.current_user()?;
        Some(self.recipients.iter().any(|rec| rec.id == current_user.id))
    }
}
