use std::sync::Arc;
use twilight_model::channel::{Group, GuildChannel, PrivateChannel};

pub enum DynamicChannel {
    Guild(Arc<GuildChannel>),
    Private(Arc<PrivateChannel>),
    Group(Arc<Group>),
}
