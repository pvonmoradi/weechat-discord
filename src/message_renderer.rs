use crate::{config::Config, twilight_utils::ext::GuildChannelExt};
use std::{cell::RefCell, sync::Arc};
use twilight::{
    cache::InMemoryCache as Cache,
    model::{channel::Message, gateway::payload::MessageUpdate, id::MessageId},
};
use weechat::{buffer::BufferHandle, Weechat};

pub struct MessageRender {
    pub buffer_handle: BufferHandle,
    config: Config,
    messages: Arc<RefCell<Vec<Message>>>,
}

impl MessageRender {
    pub fn new(buffer_handle: BufferHandle, config: &Config) -> MessageRender {
        MessageRender {
            buffer_handle,
            config: config.clone(),
            messages: Arc::new(RefCell::new(Vec::new())),
        }
    }

    async fn print_msg(&self, cache: &Cache, msg: &Message, notify: bool) {
        let (prefix, body) = render_msg(cache, &self.config, &msg).await;
        self.buffer_handle
            .upgrade()
            .expect("message renderer outlived buffer")
            .print_date_tags(
                chrono::DateTime::parse_from_rfc3339(&msg.timestamp)
                    .expect("Discord returned an invalid datetime")
                    .timestamp(),
                &MessageRender::msg_tags(cache, msg, notify).await,
                &format!("{}\t{}", prefix, body),
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
            if let Some(attachments) = update.attachments {
                old_msg.attachments = attachments
            }
            if let Some(author) = update.author {
                old_msg.author = author
            }
            if let Some(content) = update.content {
                old_msg.content = content
            }
            if let Some(embeds) = update.embeds {
                old_msg.embeds = embeds
            }
            if let Some(kind) = update.kind {
                old_msg.kind = kind
            }
            if let Some(mention_everyone) = update.mention_everyone {
                old_msg.mention_everyone = mention_everyone
            }
            if let Some(mention_roles) = update.mention_roles {
                old_msg.mention_roles = mention_roles
            }
            if let Some(pinned) = update.pinned {
                old_msg.pinned = pinned
            }
            if let Some(timestamp) = update.timestamp {
                old_msg.timestamp = timestamp
            }
            if let Some(tts) = update.tts {
                old_msg.tts = tts
            }
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

pub async fn render_msg(cache: &Cache, config: &Config, msg: &Message) -> (String, String) {
    // TODO: HACK - It seems every Message.guild_id is None
    let guild_channel = cache
        .guild_channel(msg.channel_id)
        .await
        .expect("InMemoryCache cannot fail");
    let guild_id = guild_channel.map(|ch| ch.guild_id());

    let mut msg_content =
        crate::twilight_utils::content::clean_all(cache, guild_id, &msg.content).await;

    if msg.edited_timestamp.is_some() {
        let edited_text = format!("{} (edited{}", Weechat::color("8"), Weechat::color("reset"));
        msg_content.push_str(&edited_text);
    }

    for attachment in &msg.attachments {
        if !msg_content.is_empty() {
            msg_content.push('\n');
        }
        msg_content.push_str(&attachment.proxy_url);
    }

    for embed in &msg.embeds {
        if !msg_content.is_empty() {
            msg_content.push('\n');
        }
        if let Some(ref author) = embed.author {
            msg_content.push('▎');
            msg_content.push_str(&format!(
                "{}{}{}",
                Weechat::color("bold"),
                // TODO: Should we do something else here if None?
                author.name.clone().unwrap_or_default(),
                Weechat::color("reset"),
            ));
            if let Some(url) = &author.url {
                msg_content.push_str(&format!(" ({})", url));
            }
            msg_content.push('\n');
        }
        if let Some(ref title) = embed.title {
            msg_content.push_str(
                &title
                    .lines()
                    .fold(String::new(), |acc, x| format!("{}▎{}\n", acc, x)),
            );
            msg_content.push('\n');
        }
        if let Some(ref description) = embed.description {
            msg_content.push_str(
                &description
                    .lines()
                    .fold(String::new(), |acc, x| format!("{}▎{}\n", acc, x)),
            );
            msg_content.push('\n');
        }
        for field in &embed.fields {
            msg_content.push_str(&field.name);
            msg_content.push_str(
                &field
                    .value
                    .lines()
                    .fold(String::new(), |acc, x| format!("{}▎{}\n", acc, x)),
            );
            msg_content.push('\n');
        }
        if let Some(ref footer) = embed.footer {
            msg_content.push_str(
                &footer
                    .text
                    .lines()
                    .fold(String::new(), |acc, x| format!("{}▎{}\n", acc, x)),
            );
            msg_content.push('\n');
        }
    }

    let mut prefix = String::new();

    prefix.push_str(&crate::utils::color::colorize_string(
        &config.nick_prefix(),
        &config.nick_prefix_color(),
    ));

    let author = (|| async {
        let guild_id = guild_id?;

        let member = cache
            .member(guild_id, msg.author.id)
            .await
            .expect("InMemoryCache cannot fail")?;

        Some(crate::utils::color::colorize_discord_member(cache, &member, false).await)
    })()
    .await
    .unwrap_or_else(|| msg.author.name.clone());

    prefix.push_str(&author);

    prefix.push_str(&crate::utils::color::colorize_string(
        &config.nick_suffix(),
        &config.nick_suffix_color(),
    ));

    use twilight::model::channel::message::MessageType::*;
    if let Regular = msg.kind {
        (prefix, crate::format::discord_to_weechat(&msg_content))
    } else {
        let (prefix, body) = match msg.kind {
            RecipientAdd | GuildMemberJoin => ("join", format!("{} joined the group.", author)),
            RecipientRemove => ("quit", format!("{} left the group.", author)),
            ChannelNameChange => (
                "network",
                format!("{} changed the channel name: {}.", author, msg.content),
            ),
            Call => ("network", format!("{} started a call.", author)),
            ChannelIconChange => ("network", format!("{} changed the channel icon.", author)),
            ChannelMessagePinned => (
                "network",
                format!("{} pinned a message to this channel", author),
            ),
            UserPremiumSub => (
                "network",
                format!("{} boosted this channel with nitro", author),
            ),
            UserPremiumSubTier1 => (
                "network",
                "This channel has achieved nitro level 1".to_string(),
            ),
            UserPremiumSubTier2 => (
                "network",
                "This channel has achieved nitro level 2".to_string(),
            ),
            UserPremiumSubTier3 => (
                "network",
                "This channel has achieved nitro level 3".to_string(),
            ),
            // TODO: What do these mean?
            GuildDiscoveryDisqualified => (
                "network",
                "This server has been disqualified from Discovery".to_string(),
            ),
            GuildDiscoveryRequalified => (
                "network",
                "This server has been requalified for Discovery".to_string(),
            ),
            ChannelFollowAdd => ("network", "This server has a new follow".to_string()),
            Regular => unreachable!(),
        };
        (Weechat::prefix(&prefix).to_owned(), body)
    }
}
