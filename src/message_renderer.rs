#[cfg(feature = "images")]
use crate::utils::image::*;
use crate::{
    config::Config,
    discord::discord_connection::ConnectionInner,
    match_map,
    twilight_utils::ext::MessageExt,
    weechat2::{MessageRenderer, WeechatMessage},
};
#[cfg(feature = "images")]
use image::DynamicImage;
use std::rc::Rc;
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
    Text(DiscordMessage),
    #[cfg(feature = "images")]
    Image {
        images: Vec<LoadedImage>,
        msg: DiscordMessage,
    },
}

impl Message {
    pub fn new(msg: DiscordMessage) -> Self {
        Self::Text(msg)
    }

    pub fn new_echo(author: String, content: String, nonce: u64) -> Self {
        Self::LocalEcho {
            author,
            content,
            timestamp: chrono::Utc::now().timestamp(),
            nonce,
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
            Message::Image { msg, .. } => discord_msg_tags(msg),
            Message::Text(msg) => discord_msg_tags(msg),
            Message::LocalEcho { .. } => {},
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
        match self {
            Message::LocalEcho { nonce, .. } => MessageId(*nonce),
            Message::Text(msg) => msg.id,
            #[cfg(feature = "images")]
            Message::Image { msg, .. } => msg.id,
        }
    }
}

pub struct State {
    conn: ConnectionInner,
    config: Config,
    unknown_members: Vec<UserId>,
}

pub struct WeechatRenderer {
    renderer: MessageRenderer<Message, MessageId, State>,
    conn: ConnectionInner,
}

impl WeechatRenderer {
    pub fn new(
        connection: &ConnectionInner,
        buffer_handle: Rc<BufferHandle>,
        config: &Config,
    ) -> Self {
        Self {
            renderer: MessageRenderer::new(
                buffer_handle,
                config.max_buffer_messages() as usize,
                State {
                    conn: connection.clone(),
                    config: config.clone(),
                    unknown_members: Vec::new(),
                },
            ),
            conn: connection.clone(),
        }
    }

    pub fn buffer_handle(&self) -> Rc<BufferHandle> {
        self.renderer.buffer_handle()
    }

    /// Clear the buffer and reprint all messages
    pub fn redraw_buffer(&self, ignore_users: &[UserId]) {
        self.renderer.state().borrow_mut().unknown_members.clear();

        self.renderer.redraw_buffer();

        let state = self.renderer.state();
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

        if let Some(first_msg) = self.renderer.messages().borrow().front() {
            let unknown_members = &state.borrow().unknown_members;
            if !unknown_members.is_empty() {
                if let Message::Text(first_msg) = first_msg {
                    if let Some(guild_id) = first_msg.guild_id {
                        self.fetch_guild_members(unknown_members, first_msg.channel_id, guild_id);
                    }
                }
            }
        }
    }

    pub fn add_bulk_msgs(&self, msgs: impl DoubleEndedIterator<Item = DiscordMessage>) {
        self.renderer.state().borrow_mut().unknown_members.clear();

        let mut msgs = msgs.into_iter().peekable();
        let guild_id = msgs
            .peek()
            .and_then(|msg| msg.guild_id.map(|g| (g, msg.channel_id)));

        let msgs = msgs.map(|msg| {
            #[cfg(feature = "images")]
            self.load_images(&msg);

            Message::new(msg)
        });

        self.renderer.add_bulk_msgs(msgs.into_iter());

        if let Some((guild_id, channel_id)) = guild_id {
            self.fetch_guild_members(
                &self.renderer.state().borrow().unknown_members,
                channel_id,
                guild_id,
            );
        }
    }

    #[cfg(feature = "images")]
    fn load_images(&self, msg: &DiscordMessage) {
        for candidate in find_image_candidates(&msg) {
            let renderer = self.renderer.clone();
            let rt = self.conn.rt.clone();
            let msg_id = msg.id;
            Weechat::spawn(async move {
                if let Some(image) = fetch_inline_image(&rt, &candidate.url).await {
                    let image = resize_image(
                        &image,
                        (4, 8),
                        (candidate.height as u16 / 6, candidate.width as u16 / 6),
                    );
                    renderer.update_message(msg_id, |msg| {
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
                }
            })
            .detach();
        }
    }

    pub fn add_local_echo(&self, author: String, content: String, nonce: u64) {
        self.renderer
            .add_msg(Message::new_echo(author, content, nonce), false)
    }

    pub fn add_msg(&self, msg: DiscordMessage, notify: bool) {
        if let Some(incoming_nonce) = msg.nonce.as_ref().and_then(|n| n.parse::<u64>().ok()) {
            let echo_index = self
                .renderer
                .messages()
                .borrow()
                .iter()
                .flat_map(|msg| match_map!(msg, *nonce, Message::LocalEcho { nonce, .. }))
                .position(|msg_nonce| msg_nonce == incoming_nonce);

            if let Some(echo_index) = echo_index {
                self.renderer.remove(echo_index);
                self.redraw_buffer(&[]);
            }
        }

        #[cfg(feature = "images")]
        self.load_images(&msg);

        self.renderer.state().borrow_mut().unknown_members.clear();

        self.renderer.add_msg(Message::new(msg.clone()), notify);

        if let Some(guild_id) = msg.guild_id {
            self.fetch_guild_members(
                &self.renderer.state().borrow().unknown_members,
                msg.channel_id,
                guild_id,
            );
        }
    }

    pub fn update_message<F>(&self, id: MessageId, f: F)
    where
        F: FnOnce(&mut DiscordMessage),
    {
        self.renderer.update_message(id, |msg| match msg {
            Message::LocalEcho { .. } => {},
            Message::Text(msg) => f(msg),
            #[cfg(feature = "images")]
            Message::Image { msg, .. } => f(msg),
        })
    }

    pub fn get_nth_message(&self, index: usize) -> Option<Message> {
        self.renderer.get_nth_message(index)
    }

    pub fn remove_msg(&self, id: MessageId) {
        self.renderer.remove_msg(id)
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

    if !msg.reactions.is_empty() {
        msg_content.push_str(&format!(" {}", Weechat::color("8")));
    }

    msg_content.push_str(
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
        msg_content.push_str(&Weechat::color("-8"));
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
    match msg.kind {
        Regular => (prefix, crate::utils::discord_to_weechat(&msg_content)),
        Reply if msg.referenced_message.is_none() => {
            (prefix, crate::utils::discord_to_weechat(&msg_content))
        },
        Reply => match msg.referenced_message.as_ref() {
            Some(ref_msg) => {
                let (ref_prefix, ref_msg_content) =
                    render_msg(cache, config, ref_msg, &mut Vec::new());
                let ref_msg_content = ref_msg_content
                    .lines()
                    .fold(String::new(), |acc, x| format!("{}▎{}\n", acc, x));
                (
                    prefix,
                    format!(
                        "{}:\n{}{}",
                        ref_prefix,
                        ref_msg_content,
                        crate::utils::discord_to_weechat(&msg_content)
                    ),
                )
            },
            // TODO: Currently never called due to the first Reply block above
            //       Nested replies contain only ids, so cache lookup is needed
            None => (
                prefix,
                format!(
                    "<nested reply>\n{}",
                    crate::utils::discord_to_weechat(&msg_content)
                ),
            ),
        },
        _ => {
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
                    "This channel has achieved nitro level 1".to_string(),
                ),
                UserPremiumSubTier2 => (
                    weechat::Prefix::Network,
                    "This channel has achieved nitro level 2".to_string(),
                ),
                UserPremiumSubTier3 => (
                    weechat::Prefix::Network,
                    "This channel has achieved nitro level 3".to_string(),
                ),
                // TODO: What do these mean?
                GuildDiscoveryDisqualified => (
                    weechat::Prefix::Network,
                    "This server has been disqualified from Discovery".to_string(),
                ),
                GuildDiscoveryRequalified => (
                    weechat::Prefix::Network,
                    "This server has been requalified for Discovery".to_string(),
                ),
                ChannelFollowAdd => (
                    weechat::Prefix::Network,
                    format!("This channel is now following {}", bold(&msg.content)),
                ),
                Regular | Reply => unreachable!(),
            };
            (Weechat::prefix(prefix), body)
        },
    }
}

fn bold(body: &str) -> String {
    Weechat::color("bold").to_string() + body + Weechat::color("-bold")
}
