use crate::{
    config::Config, discord::discord_connection::ConnectionInner, refcell::RefCell,
    twilight_utils::ext::MessageExt,
};
use std::{rc::Rc, sync::Arc};
use twilight_cache_inmemory::InMemoryCache as Cache;
use twilight_model::{
    channel::Message,
    gateway::payload::{MessageUpdate, RequestGuildMembers},
    id::{ChannelId, GuildId, MessageId, UserId},
};
use weechat::{buffer::BufferHandle, Weechat};

pub struct MessageRender {
    pub buffer_handle: Rc<BufferHandle>,
    conn: ConnectionInner,
    config: Config,
    messages: Arc<RefCell<Vec<Message>>>,
}

impl MessageRender {
    pub fn new(
        connection: &ConnectionInner,
        buffer_handle: Rc<BufferHandle>,
        config: &Config,
    ) -> MessageRender {
        MessageRender {
            buffer_handle,
            conn: connection.clone(),
            config: config.clone(),
            messages: Arc::new(RefCell::new(Vec::new())),
        }
    }

    fn print_msg(
        &self,
        cache: &Cache,
        msg: &Message,
        notify: bool,
        unknown_members: &mut Vec<UserId>,
    ) {
        let (prefix, body) = render_msg(cache, &self.config, &msg, unknown_members);
        self.buffer_handle
            .upgrade()
            .expect("message renderer outlived buffer")
            .print_date_tags(
                chrono::DateTime::parse_from_rfc3339(&msg.timestamp)
                    .expect("Discord returned an invalid datetime")
                    .timestamp(),
                &MessageRender::msg_tags(cache, msg, notify),
                &format!("{}\t{}", prefix, body),
            );
    }

    /// Clear the buffer and reprint all messages
    pub fn redraw_buffer(&self, cache: &Cache, ignore_users: &[UserId]) {
        self.buffer_handle
            .upgrade()
            .expect("message renderer outlived buffer")
            .clear();
        let mut unknown_members = Vec::new();
        for message in self.messages.borrow().iter() {
            self.print_msg(cache, &message, false, &mut unknown_members);
        }

        for user in ignore_users {
            // TODO: Vec::remove_item when it stabilizes
            // TODO: Make unknown_members a hashset?
            if let Some(pos) = unknown_members.iter().position(|x| x == user) {
                unknown_members.remove(pos);
            }
        }

        if let Some(first_msg) = self.messages.borrow().first() {
            if !unknown_members.is_empty() {
                if let Some(guild_id) = first_msg.guild_id {
                    self.fetch_guild_members(unknown_members, first_msg.channel_id, guild_id);
                }
            }
        }
    }

    pub fn add_bulk_msgs(&self, cache: &Cache, msgs: &[Message]) {
        let mut unknown_members = Vec::new();
        for msg in msgs {
            self.print_msg(cache, msg, false, &mut unknown_members);

            self.messages.borrow_mut().push(msg.clone());
        }

        if let Some(first_msg) = msgs.first() {
            if let Some(guild_id) = first_msg.guild_id {
                self.fetch_guild_members(unknown_members, first_msg.channel_id, guild_id);
            }
        }
    }

    pub fn add_msg(&self, cache: &Cache, msg: &Message, notify: bool) {
        let mut unknown_members = Vec::new();
        self.print_msg(cache, msg, notify, &mut unknown_members);

        self.messages.borrow_mut().push(msg.clone());

        if let Some(guild_id) = msg.guild_id {
            self.fetch_guild_members(unknown_members, msg.channel_id, guild_id);
        }
    }

    pub fn remove_msg(&self, cache: &Cache, id: MessageId) {
        let index = self.messages.borrow().iter().position(|it| it.id == id);
        if let Some(index) = index {
            self.messages.borrow_mut().remove(index);
        }
        self.redraw_buffer(cache, &[]);
    }

    pub fn update_msg(&self, cache: &Cache, update: MessageUpdate) {
        if let Some(old_msg) = self
            .messages
            .borrow_mut()
            .iter_mut()
            .find(|it| it.id == update.id)
        {
            old_msg.update(update);
        }

        self.redraw_buffer(cache, &[]);
    }

    fn msg_tags(cache: &Cache, msg: &Message, notify: bool) -> Vec<&'static str> {
        let private = cache.private_channel(msg.channel_id).is_some();

        let mentioned = cache
            .current_user()
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

    fn fetch_guild_members(
        &self,
        unknown_members: Vec<UserId>,
        channel_id: ChannelId,
        guild_id: GuildId,
    ) {
        // All messages should be the same guild and channel
        let shard = self.conn.shard.clone();
        self.conn.rt.spawn(async move {
            match shard
                .command(
                    &RequestGuildMembers::builder(guild_id)
                        .presences(true)
                        .nonce(channel_id.0.to_string())
                        .user_ids(unknown_members.into_iter().take(100).collect::<Vec<_>>())
                        .expect("input is limited to 100 members"),
                )
                .await
            {
                Err(e) => tracing::warn!(
                    guild.id = guild_id.0,
                    channel.id = guild_id.0,
                    "Failed to request guild member: {:#?}",
                    e
                ),
                Ok(_) => tracing::trace!(
                    guild.id = guild_id.0,
                    channel.id = guild_id.0,
                    "Requested guild members"
                ),
            }
        });
    }
}

fn render_msg(
    cache: &Cache,
    config: &Config,
    msg: &Message,
    unknown_members: &mut Vec<UserId>,
) -> (String, String) {
    let mut msg_content = crate::twilight_utils::content::clean_all(
        cache,
        &msg.content,
        msg.guild_id,
        config.show_unknown_user_ids(),
        unknown_members,
    );

    if msg.edited_timestamp.is_some() {
        let edited_text = format!(
            "{} (edited){}",
            Weechat::color("8"),
            Weechat::color("reset")
        );
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
        if let Some(ref provider) = embed.provider {
            if let Some(name) = &provider.name {
                msg_content.push('▎');
                msg_content.push_str(name);
                if let Some(url) = &provider.url {
                    msg_content.push_str(&format!(" ({})", url));
                }
                msg_content.push('\n');
            }
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
                    .fold(String::new(), |acc, x| format!("{}: {}\n", acc, x)),
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

    let author = (|| {
        let guild_id = msg.guild_id?;

        let member = cache.member(guild_id, msg.author.id)?;

        Some(crate::utils::color::colorize_discord_member(
            cache, &member, false,
        ))
    })()
    .unwrap_or_else(|| msg.author.name.clone());

    prefix.push_str(&author);

    prefix.push_str(&crate::utils::color::colorize_string(
        &config.nick_suffix(),
        &config.nick_suffix_color(),
    ));

    use twilight_model::channel::message::MessageType::*;
    if let Regular = msg.kind {
        (prefix, crate::utils::discord_to_weechat(&msg_content))
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
