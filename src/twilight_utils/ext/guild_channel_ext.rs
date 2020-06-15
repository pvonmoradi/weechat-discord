use twilight::model::{
    channel::{permission_overwrite::PermissionOverwrite, ChannelType, GuildChannel},
    id::{ChannelId, GuildId},
};

pub trait GuildChannelExt {
    fn name(&self) -> &str;
    fn id(&self) -> ChannelId;
    fn guild_id(&self) -> Option<GuildId>;
    fn kind(&self) -> ChannelType;
    fn permission_overwrites(&self) -> &[PermissionOverwrite];
    fn topic(&self) -> Option<String>;
}

impl GuildChannelExt for GuildChannel {
    fn name(&self) -> &str {
        match self {
            GuildChannel::Category(c) => &c.name,
            GuildChannel::Text(c) => &c.name,
            GuildChannel::Voice(c) => &c.name,
        }
    }

    fn id(&self) -> ChannelId {
        match self {
            GuildChannel::Category(c) => c.id,
            GuildChannel::Text(c) => c.id,
            GuildChannel::Voice(c) => c.id,
        }
    }
    fn guild_id(&self) -> Option<GuildId> {
        match self {
            GuildChannel::Category(c) => c.guild_id,
            GuildChannel::Text(c) => c.guild_id,
            GuildChannel::Voice(c) => c.guild_id,
        }
    }

    fn kind(&self) -> ChannelType {
        match self {
            GuildChannel::Category(c) => c.kind,
            GuildChannel::Text(c) => c.kind,
            GuildChannel::Voice(c) => c.kind,
        }
    }

    fn permission_overwrites(&self) -> &[PermissionOverwrite] {
        match self {
            GuildChannel::Category(c) => c.permission_overwrites.as_slice(),
            GuildChannel::Text(c) => c.permission_overwrites.as_slice(),
            GuildChannel::Voice(c) => c.permission_overwrites.as_slice(),
        }
    }

    fn topic(&self) -> Option<String> {
        match self {
            GuildChannel::Category(_) => None,
            GuildChannel::Text(c) => c.topic.clone(),
            GuildChannel::Voice(_) => None,
        }
    }
}
