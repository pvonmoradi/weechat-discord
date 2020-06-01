use serenity::model::prelude::*;

pub enum PluginMessage {
    Connected {
        user: CurrentUser,
    },
    SerenityError {
        message: String,
        error: serenity::Error,
    },
}
