use serde::Serialize;
use std::collections::HashMap;
use twilight_model::id::{ChannelId, GuildId, UserId};

#[derive(Serialize)]
pub struct GuildSubscriptionInfo {
    pub guild_id: GuildId,
    pub typing: bool,
    pub activities: bool,
    pub members: Vec<UserId>,
    pub channels: HashMap<ChannelId, Vec<Vec<u8>>>,
}

#[derive(Serialize)]
pub struct GuildSubscription {
    pub d: GuildSubscriptionInfo,
    pub op: u8,
}
