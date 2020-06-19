use twilight::model::{
    channel::Message,
    gateway::payload::{MemberChunk, MessageDelete, MessageUpdate},
    user::CurrentUser,
};

pub enum PluginMessage {
    Connected { user: CurrentUser },
    MessageCreate { message: Box<Message> },
    MessageDelete { event: MessageDelete },
    MessageUpdate { message: Box<MessageUpdate> },
    MemberChunk(MemberChunk),
}
