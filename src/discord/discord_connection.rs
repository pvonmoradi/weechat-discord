use crate::{
    buffer::ext::BufferExt,
    config::Config,
    discord::{plugin_message::PluginMessage, typing_indicator::TypingEntry},
    instance::Instance,
    refcell::{Ref, RefCell},
    twilight_utils::ext::{MemberExt, MessageExt, UserExt},
};
use anyhow::Result;
use once_cell::sync::Lazy;
use std::{
    collections::{HashMap, HashSet},
    error::Error,
    sync::Arc,
    time::Duration,
};
use tokio::{
    runtime::Runtime,
    stream::StreamExt,
    sync::{
        mpsc::{Receiver, Sender},
        oneshot::channel,
        Mutex as TokioMutex,
    },
};
use twilight_cache_inmemory::InMemoryCache as Cache;
use twilight_gateway::{Event as GatewayEvent, Intents, Shard};
use twilight_http::Client as HttpClient;
use twilight_model::id::{ChannelId, GuildId};
use weechat::Weechat;

#[derive(Clone, Debug)]
pub struct ConnectionInner {
    pub shard: Shard,
    pub rt: Arc<Runtime>,
    pub cache: Cache,
    pub http: HttpClient,
}

#[derive(Clone)]
pub struct DiscordConnection(Arc<RefCell<Option<ConnectionInner>>>);

impl DiscordConnection {
    pub fn new() -> Self {
        Self(Arc::new(RefCell::new(None)))
    }

    pub fn borrow(&self) -> Ref<'_, Option<ConnectionInner>> {
        self.0.borrow()
    }

    pub async fn start(&self, token: &str, tx: Sender<PluginMessage>) -> Result<ConnectionInner> {
        let (cache_tx, cache_rx) = channel();
        let runtime = Arc::new(Runtime::new().expect("Unable to create tokio runtime"));
        let token = token.to_owned();
        {
            let tx = tx.clone();
            runtime.spawn(async move {
                let mut shard = Shard::new(&token, Intents::all());
                if let Err(e) = shard.start().await {
                    let err_msg = format!("An error occurred connecting to Discord: {}", e);
                    Weechat::spawn_from_thread(async move { Weechat::print(&err_msg) });

                    // Check if the error is a 401 Unauthorized, which is likely an invalid token
                    if let Some(twilight_http::error::Error::Response { status, .. }) = e
                        .source()
                        .and_then(|e| e.downcast_ref::<twilight_http::error::Error>())
                    {
                        if status.as_u16() == 401 {
                            Weechat::spawn_from_thread(async move {
                                Weechat::print(
                                    "discord: unauthorized: check that your token is valid",
                                )
                            });
                        }
                    }

                    tracing::error!("An error occurred connecting to Discord: {:#?}", e);
                    return;
                };

                let shard = shard;

                let cache = Cache::new();

                tracing::info!("Connected to Discord");
                let mut events = shard.events();

                let http = shard.config().http_client();
                cache_tx
                    .send((shard.clone(), cache.clone(), http.clone()))
                    .map_err(|_| ())
                    .expect("Cache receiver closed before data could be sent");

                while let Some(event) = events.next().await {
                    cache.update(&event);

                    tokio::spawn(Self::handle_gateway_event(event, tx.clone()));
                }
            });
        }

        let (shard, cache, http) = cache_rx
            .await
            .map_err(|_| anyhow::anyhow!("The connection to discord failed"))?;

        let meta = ConnectionInner {
            shard,
            rt: runtime,
            cache,
            http,
        };

        self.0.borrow_mut().replace(meta.clone());

        Ok(meta)
    }

    pub fn shutdown(&self) {
        if let Some(inner) = self.0.borrow_mut().take() {
            inner.shard.shutdown();
        }
    }

    pub async fn send_guild_subscription(&self, guild_id: GuildId, channel_id: ChannelId) {
        let inner = self.0.borrow().as_ref().cloned();
        if let Some(inner) = inner {
            static CHANNELS: Lazy<TokioMutex<HashMap<GuildId, HashSet<ChannelId>>>> =
                Lazy::new(|| TokioMutex::new(HashMap::new()));

            let mut channels = CHANNELS.lock().await;
            let send = if let Some(guild_channels) = channels.get_mut(&guild_id) {
                guild_channels.insert(channel_id)
            } else {
                channels.insert(guild_id, vec![channel_id].into_iter().collect());
                true
            };

            if send {
                let channels = channels.get(&guild_id).unwrap();
                let channels_obj = channels.iter().map(|&ch| (ch, vec![vec![0, 99]])).collect();
                if let Err(e) = inner
                    .shard
                    .command(&super::custom_commands::GuildSubscription {
                        d: super::custom_commands::GuildSubscriptionInfo {
                            guild_id,
                            typing: true,
                            activities: true,
                            members: vec![],
                            channels: channels_obj,
                        },
                        op: 14,
                    })
                    .await
                {
                    tracing::warn!(guild.id=?guild_id, channel.id=?channel_id, "Unable to send guild subscription (14): {}", e);
                }
            }
        }
    }

    // Runs on weechat runtime
    pub async fn handle_events(
        mut rx: Receiver<PluginMessage>,
        conn: &ConnectionInner,
        config: Config,
        instance: Instance,
    ) {
        loop {
            let event = match rx.recv().await {
                Some(e) => e,
                None => {
                    Weechat::print("discord: error receiving message");
                    return;
                },
            };

            match event {
                PluginMessage::Connected { user } => {
                    Weechat::print(&format!("discord: ready as: {}", user.tag()));
                    tracing::info!("Ready as {}", user.tag());

                    for (guild_id, guild_config) in config.guilds() {
                        let guild = crate::buffer::guild::Guild::new(
                            guild_id,
                            conn.clone(),
                            guild_config.clone(),
                            &config,
                        );
                        if guild_config.autoconnect() {
                            if let Err(e) = guild.connect(instance.clone()) {
                                tracing::warn!("Unable to connect guild: {}", e);
                            };
                        }
                        instance.borrow_guilds_mut().insert(guild_id, guild);
                    }

                    for channel_id in config.autojoin_private() {
                        if let Some(channel) = conn.cache.private_channel(channel_id) {
                            let instance_async = instance.clone();
                            if let Ok(channel) = crate::buffer::channel::Channel::private(
                                &channel,
                                &conn,
                                &config,
                                move |_| {
                                    if let Ok(mut channels) =
                                        instance_async.try_borrow_private_channels_mut()
                                    {
                                        if let Some(channel) = channels.remove(&channel_id) {
                                            channel.set_closed();
                                        }
                                    }
                                },
                            ) {
                                instance
                                    .borrow_private_channels_mut()
                                    .insert(channel_id, channel);
                            }
                        } else {
                            tracing::warn!("Unable to find channel: {}", channel_id)
                        }
                    }
                },
                PluginMessage::MessageCreate { message } => {
                    if let Some(guild_id) = message.guild_id {
                        let channels = match instance.borrow_guilds().get(&guild_id) {
                            Some(guild) => guild.channels(),
                            None => continue,
                        };

                        let channel = match channels.get(&message.channel_id) {
                            Some(channel) => channel,
                            None => continue,
                        };

                        channel.add_message(&conn.cache, &message, !message.is_own(&conn.cache));
                    } else {
                        let private_channels = instance.borrow_private_channels_mut();
                        let channel = match private_channels.get(&message.channel_id) {
                            Some(channel) => channel,
                            None => continue,
                        };

                        channel.add_message(&conn.cache, &message, !message.is_own(&conn.cache));
                    }
                },
                PluginMessage::MessageDelete { event } => {
                    if let Some(guild_id) = event.guild_id {
                        let channels = match instance.borrow_guilds().get(&guild_id) {
                            Some(guild) => guild.channels(),
                            None => continue,
                        };

                        let channel = match channels.get(&event.channel_id) {
                            Some(channel) => channel,
                            None => continue,
                        };

                        channel.remove_message(&conn.cache, event.id);
                    } else {
                        let private_channels = instance.borrow_private_channels_mut();
                        let channel = match private_channels.get(&event.channel_id) {
                            Some(channel) => channel,
                            None => continue,
                        };

                        channel.remove_message(&conn.cache, event.id);
                    }
                },
                PluginMessage::MessageUpdate { message } => {
                    if let Some(guild_id) = message.guild_id {
                        let channels = match instance.borrow_guilds().get(&guild_id) {
                            Some(guild) => guild.channels(),
                            None => continue,
                        };

                        let channel = match channels.get(&message.channel_id) {
                            Some(channel) => channel,
                            None => continue,
                        };

                        channel.update_message(&conn.cache, *message);
                    } else {
                        let private_channels = instance.borrow_private_channels_mut();
                        let channel = match private_channels.get(&message.channel_id) {
                            Some(channel) => channel,
                            None => continue,
                        };

                        channel.update_message(&conn.cache, *message);
                    }
                },
                PluginMessage::MemberChunk(member_chunk) => {
                    let channel_id = member_chunk
                        .nonce
                        .and_then(|id| id.parse().ok().map(ChannelId));
                    if !member_chunk.not_found.is_empty() {
                        tracing::warn!(
                            "Member chunk included unknown users: {:?}",
                            member_chunk.not_found
                        );
                    }
                    if let Some(channel_id) = channel_id {
                        let channels = match instance.borrow_guilds().get(&member_chunk.guild_id) {
                            Some(guild) => guild.channels(),
                            None => continue,
                        };

                        let channel = match channels.get(&channel_id) {
                            Some(channel) => channel,
                            None => continue,
                        };
                        channel.redraw(&conn.cache, &member_chunk.not_found);
                    }
                },
                PluginMessage::TypingStart(typing) => {
                    if conn
                        .cache
                        .current_user()
                        .map(|current_user| current_user.id == typing.user_id)
                        .unwrap_or(true)
                    {
                        continue;
                    };
                    let typing_user_id = typing.user_id;
                    if let Some(name) = typing
                        .member
                        .map(|m| m.display_name().to_string())
                        .or_else(|| conn.cache.user(typing_user_id).map(|u| u.name.clone()))
                    {
                        instance.borrow_typing_tracker_mut().add(TypingEntry {
                            channel_id: typing.channel_id,
                            guild_id: typing.guild_id,
                            user: typing.user_id,
                            user_name: name,
                            time: typing.timestamp,
                        });
                        Weechat::bar_item_update("discord_typing");
                        let (mut tx, mut rx) = tokio::sync::mpsc::channel(1);
                        conn.rt.spawn(async move {
                            tokio::time::delay_for(Duration::from_secs(10)).await;
                            let _ = tx.send(()).await;
                        });

                        Weechat::spawn({
                            let instance = instance.clone();
                            async move {
                                rx.recv().await;
                                instance.borrow_typing_tracker_mut().sweep();
                                Weechat::bar_item_update("discord_typing");
                            }
                        });
                    }
                },
                PluginMessage::ChannelUpdate(channel_update) => {
                    if unsafe { Weechat::weechat() }.current_buffer().channel_id()
                        == Some(channel_update.0.id())
                    {
                        Weechat::bar_item_update("discord_slowmode_cooldown")
                    }
                },
                PluginMessage::ReactionAdd(reaction_add) => {
                    let reaction = reaction_add.0;
                    if let Some(guild_id) = reaction.guild_id {
                        let channels = match instance.borrow_guilds().get(&guild_id) {
                            Some(guild) => guild.channels(),
                            None => continue,
                        };

                        let channel = match channels.get(&reaction.channel_id) {
                            Some(channel) => channel,
                            None => continue,
                        };

                        channel.add_reaction(&conn.cache, reaction);
                    } else {
                        let private_channels = instance.borrow_private_channels_mut();
                        let channel = match private_channels.get(&reaction.channel_id) {
                            Some(channel) => channel,
                            None => continue,
                        };

                        channel.add_reaction(&conn.cache, reaction);
                    }
                },
                PluginMessage::ReactionRemove(reaction_remove) => {
                    let reaction = reaction_remove.0;
                    match reaction.guild_id {
                        Some(guild_id) => {
                            let channels = match instance.borrow_guilds().get(&guild_id) {
                                Some(guild) => guild.channels(),
                                None => continue,
                            };

                            let channel = match channels.get(&reaction.channel_id) {
                                Some(channel) => channel,
                                None => continue,
                            };

                            channel.remove_reaction(&conn.cache, reaction);
                        },
                        _ => {
                            let private_channels = instance.borrow_private_channels_mut();
                            let channel = match private_channels.get(&reaction.channel_id) {
                                Some(channel) => channel,
                                None => continue,
                            };

                            channel.remove_reaction(&conn.cache, reaction);
                        },
                    }
                },
            }
        }
    }

    // Runs on Tokio runtime
    async fn handle_gateway_event(event: GatewayEvent, mut tx: Sender<PluginMessage>) {
        match event {
            GatewayEvent::Ready(ready) => tx
                .send(PluginMessage::Connected { user: ready.user })
                .await
                .ok()
                .unwrap(),
            GatewayEvent::MessageCreate(message) => tx
                .send(PluginMessage::MessageCreate {
                    message: Box::new(message.0),
                })
                .await
                .ok()
                .expect("Receiving thread has died"),
            GatewayEvent::MessageDelete(event) => tx
                .send(PluginMessage::MessageDelete { event })
                .await
                .ok()
                .expect("Receiving thread has died"),
            GatewayEvent::MessageDeleteBulk(event) => {
                for id in event.ids {
                    tx.send(PluginMessage::MessageDelete {
                        event: twilight_model::gateway::payload::MessageDelete {
                            channel_id: event.channel_id,
                            guild_id: event.guild_id,
                            id,
                        },
                    })
                    .await
                    .ok()
                    .expect("Receiving thread has died")
                }
            },
            GatewayEvent::MessageUpdate(message) => tx
                .send(PluginMessage::MessageUpdate { message })
                .await
                .ok()
                .expect("Receiving thread has died"),
            GatewayEvent::MemberChunk(member_chunk) => tx
                .send(PluginMessage::MemberChunk(member_chunk))
                .await
                .ok()
                .expect("Receiving thread has died"),
            GatewayEvent::TypingStart(typing_start) => tx
                .send(PluginMessage::TypingStart(*typing_start))
                .await
                .ok()
                .expect("Receiving thread has died"),
            GatewayEvent::ChannelUpdate(channel_update) => tx
                .send(PluginMessage::ChannelUpdate(channel_update))
                .await
                .ok()
                .expect("Receiving thread has died"),
            GatewayEvent::ReactionAdd(reaction_add) => tx
                .send(PluginMessage::ReactionAdd(reaction_add))
                .await
                .ok()
                .expect("Receiving thread has died"),
            GatewayEvent::ReactionRemove(reaction_remove) => tx
                .send(PluginMessage::ReactionRemove(reaction_remove))
                .await
                .ok()
                .expect("Receiving thread has died"),
            _ => {},
        }
    }
}
