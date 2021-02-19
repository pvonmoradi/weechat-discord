use crate::twilight_utils::{
    ext::{GuildChannelExt, UserExt},
    DynamicChannel,
};
use twilight_cache_inmemory::{InMemoryCache as Cache, InMemoryCache};
use twilight_model::{
    channel::{ChannelType, Group, GuildChannel, PrivateChannel},
    id::{ChannelId, MessageId},
};
use twilight_permission_calculator::prelude::Permissions;

pub trait ChannelExt {
    fn name(&self) -> String;
    fn id(&self) -> ChannelId;
    fn kind(&self) -> ChannelType;
    fn can_send(&self, cache: &Cache) -> Option<bool>;
    fn last_message_id(&self) -> Option<MessageId>;
}

impl ChannelExt for DynamicChannel {
    fn name(&self) -> String {
        match self {
            DynamicChannel::Guild(ch) => ChannelExt::name(&**ch),
            DynamicChannel::Private(ch) => ch.name(),
            DynamicChannel::Group(ch) => ch.name(),
        }
    }

    fn id(&self) -> ChannelId {
        match self {
            DynamicChannel::Guild(ch) => ch.id(),
            DynamicChannel::Private(ch) => ch.id(),
            DynamicChannel::Group(ch) => ch.id(),
        }
    }

    fn kind(&self) -> ChannelType {
        match self {
            DynamicChannel::Guild(ch) => ch.kind(),
            DynamicChannel::Private(ch) => ch.kind(),
            DynamicChannel::Group(ch) => ch.kind(),
        }
    }

    fn can_send(&self, cache: &InMemoryCache) -> Option<bool> {
        match self {
            DynamicChannel::Guild(ch) => ch.can_send(cache),
            DynamicChannel::Private(ch) => ch.can_send(cache),
            DynamicChannel::Group(ch) => ch.can_send(cache),
        }
    }

    fn last_message_id(&self) -> Option<MessageId> {
        match self {
            DynamicChannel::Guild(ch) => GuildChannelExt::last_message_id(ch.as_ref()),
            DynamicChannel::Private(ch) => ch.last_message_id(),
            DynamicChannel::Group(ch) => ch.last_message_id(),
        }
    }
}

impl ChannelExt for GuildChannel {
    fn name(&self) -> String {
        self.name().to_owned()
    }

    fn id(&self) -> ChannelId {
        self.id()
    }

    fn kind(&self) -> ChannelType {
        match self {
            GuildChannel::Category(c) => c.kind,
            GuildChannel::Text(c) => c.kind,
            GuildChannel::Voice(c) => c.kind,
        }
    }

    fn can_send(&self, cache: &Cache) -> Option<bool> {
        self.has_permission(cache, Permissions::SEND_MESSAGES)
    }

    fn last_message_id(&self) -> Option<MessageId> {
        GuildChannelExt::last_message_id(self)
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

    fn can_send(&self, _cache: &Cache) -> Option<bool> {
        Some(true)
    }

    fn last_message_id(&self) -> Option<MessageId> {
        self.last_message_id
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

    fn last_message_id(&self) -> Option<MessageId> {
        self.last_message_id
    }
}
