use twilight_model::{
    channel::Message,
    gateway::payload::{ChannelUpdate, MemberChunk, MessageDelete, MessageUpdate, TypingStart},
    user::CurrentUser,
};

pub enum PluginMessage {
    Connected { user: CurrentUser },
    MessageCreate { message: Box<Message> },
    MessageDelete { event: MessageDelete },
    MessageUpdate { message: Box<MessageUpdate> },
    MemberChunk(MemberChunk),
    TypingStart(TypingStart),
    ChannelUpdate(ChannelUpdate),
}
