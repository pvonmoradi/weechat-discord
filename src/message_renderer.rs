use std::{cell::RefCell, sync::Arc};
use twilight::{
    cache::InMemoryCache as Cache,
    model::{channel::Message, gateway::payload::MessageUpdate, id::MessageId},
};
use weechat::buffer::BufferHandle;

pub struct MessageRender {
    pub buffer_handle: BufferHandle,
    messages: Arc<RefCell<Vec<Message>>>,
}

impl MessageRender {
    pub fn new(buffer_handle: BufferHandle) -> MessageRender {
        MessageRender {
            buffer_handle,
            messages: Arc::new(RefCell::new(Vec::new())),
        }
    }

    async fn print_msg(&self, cache: &Cache, msg: &Message, notify: bool) {
        self.buffer_handle
            .upgrade()
            .expect("message renderer outlived buffer")
            .print_date_tags(
                chrono::DateTime::parse_from_rfc3339(&msg.timestamp)
                    .expect("Discord returned an invalid datetime")
                    .timestamp(),
                &MessageRender::msg_tags(cache, msg, notify).await,
                &msg.content,
            );
    }

    /// Clear the buffer and reprint all messages
    pub async fn redraw_buffer(&self, cache: &Cache) {
        self.buffer_handle
            .upgrade()
            .expect("message renderer outlived buffer")
            .clear();
        for message in self.messages.borrow().iter() {
            self.print_msg(cache, &message, false).await;
        }
    }

    pub async fn add_msg(&self, cache: &Cache, msg: &Message, notify: bool) {
        self.print_msg(cache, msg, notify).await;

        self.messages.borrow_mut().push(msg.clone());
    }

    pub async fn remove_msg(&self, cache: &Cache, id: MessageId) {
        let index = self.messages.borrow().iter().position(|it| it.id == id);
        if let Some(index) = index {
            self.messages.borrow_mut().remove(index);
        }
        self.redraw_buffer(cache).await;
    }

    pub async fn update_msg(&self, cache: &Cache, update: MessageUpdate) {
        if let Some(old_msg) = self
            .messages
            .borrow_mut()
            .iter_mut()
            .find(|it| it.id == update.id)
        {
            old_msg.id = update.id;
            old_msg.channel_id = update.channel_id;
            old_msg.edited_timestamp = update.edited_timestamp;
            for user in update.mentions.unwrap_or_default() {
                old_msg.mentions.insert(user.id, user);
            }
            update
                .attachments
                .map(|attachments| old_msg.attachments = attachments);
            update.author.map(|author| old_msg.author = author);
            update.content.map(|content| old_msg.content = content);
            update.embeds.map(|embeds| old_msg.embeds = embeds);
            update.kind.map(|kind| old_msg.kind = kind);
            update
                .mention_everyone
                .map(|mention_everyone| old_msg.mention_everyone = mention_everyone);
            update
                .mention_roles
                .map(|mention_roles| old_msg.mention_roles = mention_roles);

            update.pinned.map(|pinned| old_msg.pinned = pinned);
            update
                .timestamp
                .map(|timestamp| old_msg.timestamp = timestamp);
            update.tts.map(|tts| old_msg.tts = tts);
        }

        self.redraw_buffer(cache).await;
    }

    async fn msg_tags(cache: &Cache, msg: &Message, notify: bool) -> Vec<&'static str> {
        let private = cache
            .private_channel(msg.channel_id)
            .await
            .expect("InMemoryCache cannot fail")
            .is_some();

        let mentioned = cache
            .current_user()
            .await
            .expect("InMemoryCache cannot fail")
            .map(|user| msg.mentions.contains_key(&user.id))
            .unwrap_or(false);

        let mut tags = Vec::new();
        if notify {
            if mentioned {
                tags.push("notify_highlight");
            }

            if private {
                tags.push("notify_private");
            }

            if !(mentioned || private) {
                tags.push("notify_message");
            }
        } else {
            tags.push("notify_none");
        }

        tags
    }
}
