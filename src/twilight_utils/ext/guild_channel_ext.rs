use twilight::model::{
    channel::{permission_overwrite::PermissionOverwrite, GuildChannel},
    id::GuildId,
};

pub trait GuildChannelExt {
    fn guild_id(&self) -> GuildId;
    fn permission_overwrites(&self) -> &[PermissionOverwrite];
    fn topic(&self) -> Option<String>;
}

impl GuildChannelExt for GuildChannel {
    fn guild_id(&self) -> GuildId {
        match self {
            GuildChannel::Category(c) => c.guild_id.expect("GuildChannel must have a guild_id"),
            GuildChannel::Text(c) => c.guild_id.expect("GuildChannel must have a guild_id"),
            GuildChannel::Voice(c) => c.guild_id.expect("GuildChannel must have a guild_id"),
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
