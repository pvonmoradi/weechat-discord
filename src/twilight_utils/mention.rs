use twilight::{
    cache_inmemory::model::CachedEmoji,
    model::id::{ChannelId, RoleId, UserId},
};

/// Allows something - such as a channel or role - to be mentioned in a message.
pub trait Mentionable {
    /// Creates a mentionable string, that will be able to notify and/or create
    /// a link to the item.
    fn mention(&self) -> String;
}

impl Mentionable for ChannelId {
    fn mention(&self) -> String {
        format!("<#{}>", self.0)
    }
}

impl Mentionable for UserId {
    fn mention(&self) -> String {
        format!("<@{}>", self.0)
    }
}

impl Mentionable for RoleId {
    fn mention(&self) -> String {
        format!("<@&{}>", self.0)
    }
}

impl Mentionable for CachedEmoji {
    fn mention(&self) -> String {
        format!("<:{}:{}>", self.name, self.id.0)
    }
}
