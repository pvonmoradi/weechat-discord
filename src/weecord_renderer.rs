#[cfg(feature = "images")]
use crate::utils::image::*;
use crate::{
    config::Config,
    discord::discord_connection::ConnectionInner,
    match_map,
    twilight_utils::ext::MessageExt,
    utils::fold_lines,
    weechat2::{MessageRenderer, Style, StyledString, WeechatMessage},
    RefCell,
};
#[cfg(feature = "images")]
use image::DynamicImage;
use std::{collections::VecDeque, rc::Rc};
use twilight_cache_inmemory::InMemoryCache as Cache;
use twilight_model::{
    channel::{Message as DiscordMessage, ReactionType},
    gateway::payload::{MessageUpdate, RequestGuildMembers},
    id::{ChannelId, GuildId, MessageId, UserId},
};
use weechat::{buffer::BufferHandle, Weechat};

#[cfg(feature = "images")]
#[derive(Clone)]
pub struct LoadedImage {
    pub image: DynamicImage,
    pub height: u64,
    pub width: u64,
}

#[derive(Clone)]
pub enum Message {
    LocalEcho {
        author: String,
        content: String,
        timestamp: i64,
        nonce: u64,
    },
    Text(Box<DiscordMessage>),
    #[cfg(feature = "images")]
    Image {
        images: Vec<LoadedImage>,
        msg: Box<DiscordMessage>,
    },
}

impl Message {
    pub fn new(msg: DiscordMessage) -> Self {
        Self::Text(Box::new(msg))
    }

    pub fn new_echo(author: String, content: String, nonce: u64) -> Self {
        Self::LocalEcho {
            author,
            content,
            timestamp: chrono::Utc::now().timestamp(),
            nonce,
        }
    }

    pub fn id(&self) -> MessageId {
        match self {
            Message::LocalEcho { nonce, .. } => MessageId(*nonce),
            Message::Text(msg) => msg.id,
            #[cfg(feature = "images")]
            Message::Image { msg, .. } => msg.id,
        }
    }
}

impl WeechatMessage<MessageId, State> for Message {
    fn render(&self, state: &mut State) -> (String, String) {
        match self {
            Message::LocalEcho {
                author, content, ..
            } => (
                author.clone(),
                format!(
                    "{}{}{}",
                    Weechat::color("244"),
                    content,
                    Weechat::color("resetcolor")
                ),
            ),
            Message::Text(msg) => render_msg(
                &state.conn.cache,
                &state.config,
                msg,
                &mut state.unknown_members,
            ),
            #[cfg(feature = "images")]
            Message::Image { msg, images } => {
                let (prefix, mut body) = render_msg(
                    &state.conn.cache,
                    &state.config,
                    msg,
                    &mut state.unknown_members,
                );

                if !images.is_empty() {
                    body += "\n";
                }
                for image in images {
                    body += &render_img(&image.image);
                }

                (prefix, body)
            },
        }
    }

    fn tags(&self, state: &mut State, notify: bool) -> Vec<&'static str> {
        let mut tags = Vec::new();

        let mut discord_msg_tags = |msg: &DiscordMessage| {
            let cache = &state.conn.cache;
            let private = cache.private_channel(msg.channel_id).is_some();

            let mentioned = cache
                .current_user()
                .map(|user| msg.mentions.iter().any(|m| m.id == user.id))
                .unwrap_or(false);

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
            }
        };

        match self {
            #[cfg(feature = "images")]
            Message::Image { msg, .. } => {
                discord_msg_tags(msg);
                tags.push("no_log");
            },
            Message::Text(msg) => discord_msg_tags(msg),
            Message::LocalEcho { .. } => {
                tags.push("no_log");
            },
        }
        tags
    }

    fn timestamp(&self, _: &mut State) -> i64 {
        match self {
            Message::LocalEcho { timestamp, .. } => *timestamp,
            Message::Text(msg) => chrono::DateTime::parse_from_rfc3339(&msg.timestamp)
                .expect("Discord returned an invalid datetime")
                .timestamp(),
            #[cfg(feature = "images")]
            Message::Image { msg, .. } => chrono::DateTime::parse_from_rfc3339(&msg.timestamp)
                .expect("Discord returned an invalid datetime")
                .timestamp(),
        }
    }

    fn id(&self, _: &mut State) -> MessageId {
        self.id()
    }
}

pub struct State {
    conn: ConnectionInner,
    config: Config,
    unknown_members: Vec<UserId>,
}

pub struct WeecordRenderer {
    inner: MessageRenderer<Message, MessageId, State>,
    #[cfg(feature = "images")]
    config: Config,
    conn: ConnectionInner,
}

impl WeecordRenderer {
    pub fn new(
        connection: &ConnectionInner,
        buffer_handle: Rc<BufferHandle>,
        config: &Config,
    ) -> Self {
        Self {
            inner: MessageRenderer::new(
                buffer_handle,
                config.max_buffer_messages() as usize,
                State {
                    conn: connection.clone(),
                    config: config.clone(),
                    unknown_members: Vec::new(),
                },
            ),
            #[cfg(feature = "images")]
            config: config.clone(),
            conn: connection.clone(),
        }
    }

    pub fn buffer_handle(&self) -> Rc<BufferHandle> {
        self.inner.buffer_handle()
    }

    pub fn set_last_read_id(&self, id: MessageId) {
        self.inner.set_last_read_id(id);
    }
    /// Clear the buffer and reprint all messages
    pub fn redraw_buffer(&self, ignore_users: &[UserId]) {
        self.inner.state().borrow_mut().unknown_members.clear();

        self.inner.redraw_buffer();

        let state = self.inner.state();
        {
            let mut state = state.borrow_mut();
            let unknown_members = &mut state.unknown_members;
            // TODO: Use drain_filter when it stabilizes
            for user in ignore_users {
                // TODO: Make unknown_members a hashset?
                if let Some(pos) = unknown_members.iter().position(|x| x == user) {
                    unknown_members.remove(pos);
                }
            }
        }

        if let Some(Message::Text(first_msg)) = self.inner.messages().borrow().front() {
            if let Some(guild_id) = first_msg.guild_id {
                self.fetch_guild_members(
                    &state.borrow().unknown_members,
                    first_msg.channel_id,
                    guild_id,
                );
            }
        }
    }

    pub fn add_bulk_msgs(&self, msgs: impl DoubleEndedIterator<Item = DiscordMessage>) {
        self.inner.state().borrow_mut().unknown_members.clear();

        let mut msgs = msgs.into_iter().peekable();
        let guild_id = msgs
            .peek()
            .and_then(|msg| msg.guild_id.map(|g| (g, msg.channel_id)));

        let msgs = msgs.map(|msg| {
            #[cfg(feature = "images")]
            self.load_images(&msg);

            Message::new(msg)
        });

        self.inner.add_bulk_msgs(msgs.into_iter());

        if let Some((guild_id, channel_id)) = guild_id {
            self.fetch_guild_members(
                &self.inner.state().borrow().unknown_members,
                channel_id,
                guild_id,
            );
        }
    }

    #[cfg(feature = "images")]
    fn load_images(&self, msg: &DiscordMessage) {
        for candidate in find_image_candidates(&msg) {
            let renderer = self.inner.clone();
            let rt = self.conn.rt.clone();
            let msg_id = msg.id;
            let max_height = self.config.image_max_height() as u32;
            Weechat::spawn(async move {
                match fetch_inline_image(&rt, &candidate.url).await {
                    Ok(image) => {
                        let image =
                            resize_image(&image, (4, 8), (max_height, u16::max_value() as u32));
                        renderer.update_message(&msg_id, |msg| {
                            let loaded_image = LoadedImage {
                                image,
                                height: candidate.height,
                                width: candidate.width,
                            };
                            match msg {
                                Message::Text(discord_msg) => {
                                    *msg = Message::Image {
                                        images: vec![loaded_image],
                                        msg: discord_msg.clone(),
                                    }
                                },
                                Message::Image { images, .. } => images.push(loaded_image),
                                _ => {},
                            }
                        });
                        renderer.redraw_buffer();
                    },
                    Err(e) => {
                        tracing::error!("Failed to fetch image: {}", e);
                    },
                }
            })
            .detach();
        }
    }

    pub fn add_local_echo(&self, author: String, content: String, nonce: u64) {
        self.inner
            .add_msg(Message::new_echo(author, content, nonce), false);
    }

    pub fn add_msg(&self, msg: &DiscordMessage, notify: bool) {
        if let Some(incoming_nonce) = msg.nonce.as_ref().and_then(|n| n.parse::<u64>().ok()) {
            let echo_index = self
                .inner
                .messages()
                .borrow()
                .iter()
                .flat_map(|msg| match_map!(msg, Message::LocalEcho { nonce, .. } => *nonce))
                .position(|msg_nonce| msg_nonce == incoming_nonce);

            if let Some(echo_index) = echo_index {
                self.inner.remove(echo_index);
                self.redraw_buffer(&[]);
            }
        }

        #[cfg(feature = "images")]
        self.load_images(&msg);

        self.inner.state().borrow_mut().unknown_members.clear();

        self.inner.add_msg(Message::new(msg.clone()), notify);

        if let Some(guild_id) = msg.guild_id {
            self.fetch_guild_members(
                &self.inner.state().borrow().unknown_members,
                msg.channel_id,
                guild_id,
            );
        }
    }

    pub fn update_message<F>(&self, id: MessageId, f: F)
    where
        F: FnOnce(&mut DiscordMessage),
    {
        self.inner.update_message(&id, |msg| match msg {
            Message::LocalEcho { .. } => {},
            Message::Text(msg) => f(msg),
            #[cfg(feature = "images")]
            Message::Image { msg, .. } => f(msg),
        });
    }

    pub fn get_nth_message(&self, index: usize) -> Option<Message> {
        self.inner.get_nth_message(index)
    }

    pub fn nth_oldest_message(&self, index: usize) -> Option<Message> {
        self.inner.nth_oldest_message(index)
    }

    pub fn messages(&self) -> Rc<RefCell<VecDeque<Message>>> {
        self.inner.messages()
    }

    pub fn remove_msg(&self, id: MessageId) {
        self.inner.remove_msg(&id);
    }

    pub fn apply_message_update(&self, update: MessageUpdate) {
        self.update_message(update.id, |msg| msg.update(update));
        self.redraw_buffer(&[]);
    }

    fn fetch_guild_members(
        &self,
        unknown_members: &[UserId],
        channel_id: ChannelId,
        guild_id: GuildId,
    ) {
        if unknown_members.is_empty() {
            tracing::trace!("Skipping fetch_guild_members, no members requested");
            return;
        }
        // All messages should be the same guild and channel
        let conn = &self.conn;
        let shard = conn.shard.clone();
        let unknown_members = unknown_members.to_vec();
        conn.rt.spawn(async move {
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
    msg: &DiscordMessage,
    unknown_members: &mut Vec<UserId>,
) -> (String, String) {
    use twilight_model::channel::message::MessageType::*;
    let mut msg_content = crate::utils::discord_to_weechat(
        &msg.content,
        cache,
        msg.guild_id,
        config.show_unknown_user_ids(),
        unknown_members,
    );

    if msg.edited_timestamp.is_some() {
        msg_content.push_styled_str(Style::color("8"), " (edited)");
    }

    for attachment in &msg.attachments {
        if !msg_content.is_empty() {
            msg_content.push_str("\n");
        }
        msg_content.push_str(&attachment.proxy_url);
    }

    msg_content.append(format_embeds(&msg, !msg_content.is_empty()));

    msg_content.append(format_reactions(&msg));

    let (prefix, author) = format_author_prefix(cache, &config, msg);

    let prefix = prefix.build();
    let msg_content = msg_content.build();
    match msg.kind {
        Regular => (prefix, msg_content),
        Reply if msg.referenced_message.is_none() => (prefix, msg_content),
        Reply => match msg.referenced_message.as_ref() {
            Some(ref_msg) => {
                let (ref_prefix, ref_msg_content) =
                    render_msg(cache, config, ref_msg, &mut Vec::new());

                let ref_msg_content = fold_lines(ref_msg_content.lines(), "▎");
                (
                    prefix,
                    format!(
                        "{}:\n{}\n{}",
                        ref_prefix,
                        ref_msg_content.build(),
                        msg_content
                    ),
                )
            },
            // TODO: Currently never called due to the first Reply block above
            //       Nested replies contain only ids, so cache lookup is needed
            None => (prefix, format!("<nested reply>\n{}", msg_content)),
        },
        _ => format_event_message(msg, &author.build()),
    }
}

fn format_embeds(msg: &DiscordMessage, leading_newline: bool) -> StyledString {
    let mut out = StyledString::new();
    for embed in &msg.embeds {
        if leading_newline {
            out.push_str("\n");
        }
        if let Some(ref provider) = embed.provider {
            if let Some(name) = &provider.name {
                out.push_str("▎");
                out.push_str(name);
                if let Some(url) = &provider.url {
                    out.push_str(&format!(" ({})", url));
                }
                out.push_str("\n");
            }
        }
        if let Some(ref author) = embed.author {
            out.push_str("▎");
            out.push_style(Style::color("bold"));
            // TODO: Should we do something else here if None?
            out.push_str(&author.name.clone().unwrap_or_default());
            out.pop_style(Style::color("bold"));
            if let Some(url) = &author.url {
                out.push_str(&format!(" ({})", url));
            }
            out.push_str("\n");
        }
        if let Some(ref title) = embed.title {
            out.append(fold_lines(title.lines(), "▎"));

            out.push_str("\n");
        }
        if let Some(ref description) = embed.description {
            out.append(fold_lines(description.lines(), "▎"));
            out.push_str("\n");
        }
        for field in &embed.fields {
            out.push_str("▎");
            out.push_str(&field.name);
            out.push_str(": ");
            out.push_str(&field.value.lines().collect::<Vec<_>>().join(":"));
            out.push_str("\n");
        }
        if let Some(ref footer) = embed.footer {
            out.append(fold_lines(footer.text.lines(), "▎"));
            out.push_str("\n");
        }
    }

    out
}

fn format_reactions(msg: &DiscordMessage) -> StyledString {
    let mut out = StyledString::new();
    if !msg.reactions.is_empty() {
        out.push_str(" ");
        out.push_style(Style::color("8"));
    }

    out.push_str(
        &msg.reactions
            .iter()
            .flat_map(|reaction| {
                match &reaction.emoji {
                    ReactionType::Custom { name, .. } => name.clone().map(|n| format!(":{}:", n)),
                    ReactionType::Unicode { name } => Some(name.clone()),
                }
                .map(|e| format!("[{} {}]", e, reaction.count))
            })
            .collect::<Vec<_>>()
            .join(" "),
    );

    if !msg.reactions.is_empty() {
        out.push_style(Style::color("-8"));
    }

    out
}

fn format_author_prefix(
    cache: &Cache,
    config: &Config,
    msg: &DiscordMessage,
) -> (StyledString, StyledString) {
    let mut prefix = StyledString::new();

    prefix.append(crate::utils::color::colorize_string(
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
    .unwrap_or_else(|| msg.author.name.clone().into());

    prefix.append(author.clone());

    prefix.append(crate::utils::color::colorize_string(
        &config.nick_suffix(),
        &config.nick_suffix_color(),
    ));
    (prefix, author)
}

fn format_event_message(msg: &DiscordMessage, author: &str) -> (String, String) {
    use twilight_model::channel::message::MessageType::*;
    let (prefix, body) = match msg.kind {
        RecipientAdd | GuildMemberJoin => (
            weechat::Prefix::Join,
            format!("{} joined the group.", bold(&author)),
        ),
        RecipientRemove => (
            weechat::Prefix::Quit,
            format!("{} left the group.", bold(&author)),
        ),
        ChannelNameChange => (
            weechat::Prefix::Network,
            format!(
                "{} changed the channel name to {}.",
                bold(&author),
                bold(&msg.content)
            ),
        ),
        Call => (
            weechat::Prefix::Network,
            format!("{} started a call.", bold(&author)),
        ),
        ChannelIconChange => (
            weechat::Prefix::Network,
            format!("{} changed the channel icon.", bold(&author)),
        ),
        ChannelMessagePinned => (
            weechat::Prefix::Network,
            format!("{} pinned a message to this channel", bold(&author)),
        ),
        UserPremiumSub => (
            weechat::Prefix::Network,
            format!("{} boosted this channel with nitro", bold(&author)),
        ),
        UserPremiumSubTier1 => (
            weechat::Prefix::Network,
            "This channel has achieved nitro level 1".to_owned(),
        ),
        UserPremiumSubTier2 => (
            weechat::Prefix::Network,
            "This channel has achieved nitro level 2".to_owned(),
        ),
        UserPremiumSubTier3 => (
            weechat::Prefix::Network,
            "This channel has achieved nitro level 3".to_owned(),
        ),
        // TODO: What do these mean?
        GuildDiscoveryDisqualified => (
            weechat::Prefix::Network,
            "This server has been disqualified from Discovery".to_owned(),
        ),
        GuildDiscoveryRequalified => (
            weechat::Prefix::Network,
            "This server has been requalified for Discovery".to_owned(),
        ),
        ChannelFollowAdd => (
            weechat::Prefix::Network,
            format!("This channel is now following {}", bold(&msg.content)),
        ),
        Regular | Reply => unreachable!(),
    };
    (Weechat::prefix(prefix), body)
}

fn bold(body: &str) -> String {
    Weechat::color("bold").to_owned() + body + Weechat::color("-bold")
}
