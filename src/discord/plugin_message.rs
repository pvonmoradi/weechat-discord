use twilight::model::{channel::Message, user::CurrentUser};

pub enum PluginMessage {
    Connected { user: CurrentUser },
    MessageCreate { message: Message },
}
