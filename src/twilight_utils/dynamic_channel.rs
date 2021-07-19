use twilight_model::channel::{Group, GuildChannel, PrivateChannel};

pub enum DynamicChannel {
    Guild(GuildChannel),
    Private(PrivateChannel),
    Group(Group),
}
