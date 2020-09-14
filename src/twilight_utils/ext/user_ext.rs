use twilight_model::user::{CurrentUser, User};

pub trait UserExt {
    fn tag(&self) -> String;
}

impl UserExt for User {
    fn tag(&self) -> String {
        format!("{}#{:04}", self.name, self.discriminator)
    }
}

impl UserExt for CurrentUser {
    fn tag(&self) -> String {
        format!("{}#{:04}", self.name, self.discriminator)
    }
}
