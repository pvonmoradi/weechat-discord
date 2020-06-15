use async_trait::async_trait;
use twilight::{
    cache::{InMemoryCache as Cache, InMemoryCache},
    model::channel::Message,
};

#[async_trait]
pub trait MessageExt {
    async fn is_own(&self, cache: &Cache) -> bool;
}

#[async_trait]
impl MessageExt for Message {
    async fn is_own(&self, cache: &InMemoryCache) -> bool {
        let current_user = match cache
            .current_user()
            .await
            .expect("InMemoryCache cannot fail")
        {
            Some(current_user) => current_user,
            None => return false,
        };

        self.author.id == current_user.id
    }
}
