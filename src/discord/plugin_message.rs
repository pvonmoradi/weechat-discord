use twilight::model::{channel::Message, gateway::payload::MessageDelete, user::CurrentUser};

pub enum PluginMessage {
    Connected { user: CurrentUser },
    MessageCreate { message: Message },
    MessageDelete { event: MessageDelete },
}
