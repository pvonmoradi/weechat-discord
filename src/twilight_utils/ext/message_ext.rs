use async_trait::async_trait;
use twilight::{
    cache::{InMemoryCache as Cache, InMemoryCache},
    model::{channel::Message, gateway::payload::MessageUpdate},
};

#[async_trait]
pub trait MessageExt {
    async fn is_own(&self, cache: &Cache) -> bool;

    fn update(&mut self, update: MessageUpdate);
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

    fn update(&mut self, update: MessageUpdate) {
        self.id = update.id;
        self.channel_id = update.channel_id;
        self.edited_timestamp = update.edited_timestamp;
        for user in update.mentions.unwrap_or_default() {
            self.mentions.insert(user.id, user);
        }
        if let Some(attachments) = update.attachments {
            self.attachments = attachments
        }
        if let Some(author) = update.author {
            self.author = author
        }
        if let Some(content) = update.content {
            self.content = content
        }
        if let Some(embeds) = update.embeds {
            self.embeds = embeds
        }
        if let Some(kind) = update.kind {
            self.kind = kind
        }
        if let Some(mention_everyone) = update.mention_everyone {
            self.mention_everyone = mention_everyone
        }
        if let Some(mention_roles) = update.mention_roles {
            self.mention_roles = mention_roles
        }
        if let Some(pinned) = update.pinned {
            self.pinned = pinned
        }
        if let Some(timestamp) = update.timestamp {
            self.timestamp = timestamp
        }
        if let Some(tts) = update.tts {
            self.tts = tts
        }
    }
}
