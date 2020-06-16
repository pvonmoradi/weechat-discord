use twilight::model::{
    channel::Message,
    gateway::payload::{MessageDelete, MessageUpdate},
    user::CurrentUser,
};

pub enum PluginMessage {
    Connected { user: CurrentUser },
    MessageCreate { message: Message },
    MessageDelete { event: MessageDelete },
    MessageUpdate { message: Box<MessageUpdate> },
}
