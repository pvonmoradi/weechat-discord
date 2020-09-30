use crate::{
    config::Config,
    discord::plugin_message::PluginMessage,
    instance::Instance,
    refcell::{Ref, RefCell},
    twilight_utils::ext::{MessageExt, UserExt},
};
use anyhow::Result;
use std::sync::Arc;
use tokio::{
    runtime::Runtime,
    stream::StreamExt,
    sync::{
        mpsc::{Receiver, Sender},
        oneshot::channel,
    },
};

use std::error::Error;
use twilight_cache_inmemory::InMemoryCache as Cache;
use twilight_gateway::{Event as GatewayEvent, Shard};
use twilight_http::Client as HttpClient;
use twilight_model::id::ChannelId;
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
                let mut shard = Shard::new(&token);
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
                                    "Discord: Unauthorized: Check that your token is valid",
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
                    Weechat::print("Error receiving message");
                    return;
                },
            };

            match event {
                PluginMessage::Connected { user } => {
                    Weechat::print(&format!("discord: ready as: {}", user.tag()));
                    tracing::info!("Ready as {}", user.tag());

                    for (guild_id, guild_config) in config.guilds() {
                        let guild = crate::guild::Guild::new(
                            guild_id,
                            conn.clone(),
                            guild_config.clone(),
                            &config,
                        );
                        if guild_config.autoconnect() {
                            if let Err(e) = guild.connect(instance.clone()).await {
                                tracing::warn!("Unable to connect guild: {}", e);
                            };
                        }
                        instance.borrow_guilds_mut().insert(guild_id, guild);
                    }

                    for channel_id in config.autojoin_private() {
                        if let Some(channel) = conn.cache.private_channel(channel_id) {
                            let instance_async = instance.clone();
                            if let Ok(channel) = crate::channel::Channel::private(
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
            _ => {},
        }
    }
}
