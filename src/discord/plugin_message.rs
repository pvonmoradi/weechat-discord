use twilight::model::user::CurrentUser;

pub enum PluginMessage {
    Connected { user: CurrentUser },
}
