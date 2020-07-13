use crate::{
    discord::plugin_message::PluginMessage,
    twilight_utils::ext::{GuildChannelExt, MessageExt},
    DiscordSession,
};
use anyhow::Result;
use std::{
    cell::{Ref, RefCell},
    rc::Rc,
    sync::Arc,
};
use tokio::{
    runtime::Runtime,
    stream::StreamExt,
    sync::{
        mpsc::{Receiver, Sender},
        oneshot::channel,
    },
};
use tracing::*;
use twilight::{
    cache::InMemoryCache as Cache,
    gateway::{Event as GatewayEvent, Shard, ShardConfig},
    http::Client as HttpClient,
    model::id::ChannelId,
};
use weechat::Weechat;

#[derive(Clone)]
pub struct ConnectionMeta {
    pub shard: Shard,
    pub rt: Arc<Runtime>,
    pub cache: Cache,
    pub http: HttpClient,
}

#[derive(Clone)]
pub struct DiscordConnection(Rc<RefCell<Option<ConnectionMeta>>>);

impl DiscordConnection {
    pub fn new() -> Self {
        Self(Rc::new(RefCell::new(None)))
    }

    pub fn borrow(&self) -> Ref<'_, Option<ConnectionMeta>> {
        self.0.borrow()
    }

    pub async fn start(&self, token: &str, tx: Sender<PluginMessage>) -> Result<ConnectionMeta> {
        let (cache_tx, cache_rx) = channel();
        let runtime = Arc::new(Runtime::new().expect("Unable to create tokio runtime"));
        let token = token.to_owned();
        {
            let tx = tx.clone();
            runtime.spawn(async move {
                let config = ShardConfig::builder(&token).build();
                let mut shard = Shard::new(config);
                if let Err(e) = shard.start().await {
                    let err_msg = format!("An error occured connecting to Discord: {}", e);
                    Weechat::spawn_from_thread(async move { Weechat::print(&err_msg) });
                    error!("An error occured connecting to Discord: {:#?}", e);
                    return;
                };

                let shard = shard;

                let cache = Cache::new();

                info!("Connected to Discord");
                let mut events = shard.events().await;

                let http = shard.config().http_client();
                cache_tx
                    .send((shard.clone(), cache.clone(), http.clone()))
                    .map_err(|_| ())
                    .expect("Cache receiver closed before data could be sent");

                while let Some(event) = events.next().await {
                    cache
                        .update(&event)
                        .await
                        .expect("InMemoryCache cannot error");

                    tokio::spawn(Self::handle_gateway_event(event, tx.clone()));
                }
            });
        }

        let (shard, cache, http) = cache_rx
            .await
            .map_err(|_| anyhow::anyhow!("The connection to discord failed"))?;

        let meta = ConnectionMeta {
            shard,
            rt: runtime,
            cache,
            http,
        };

        self.0.borrow_mut().replace(meta.clone());

        Ok(meta)
    }

    pub async fn handle_events(
        mut rx: Receiver<PluginMessage>,
        session: DiscordSession,
        conn: &ConnectionMeta,
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
                    Weechat::print(&format!(
                        "discord: ready as: {}#{:04}",
                        user.name, user.discriminator
                    ));

                    for (guild_id, guild) in session.guilds.borrow().iter() {
                        if guild.autoconnect() {
                            trace!(guild_id = guild_id.0, "Autoconnecting");
                            if let Err(e) = guild.connect(conn, session.guilds.clone()).await {
                                warn!(guild_id = guild_id.0, error=?e, "Error connecting guild");
                            }
                        }
                    }
                },
                PluginMessage::MessageCreate { message } => {
                    if let Some(guild_id) = message.guild_id {
                        let buffers = {
                            let guilds = session.guilds.borrow();
                            match guilds.get(&guild_id) {
                                Some(guild) => guild.channel_buffers(),
                                None => continue,
                            }
                        };

                        let channel = match buffers.get(&message.channel_id) {
                            Some(channel) => channel,
                            None => continue,
                        };

                        channel
                            .add_message(&conn.cache, &message, message.is_own(&conn.cache).await)
                            .await;
                    }
                },
                PluginMessage::MessageDelete { event } => {
                    if let Some(guild_channel) = conn
                        .cache
                        .guild_channel(event.channel_id)
                        .await
                        .expect("InMemoryCache cannot fail")
                    {
                        let buffers = {
                            let guilds = session.guilds.borrow();
                            match guilds.get(&guild_channel.guild_id()) {
                                Some(guild) => guild.channel_buffers(),
                                None => continue,
                            }
                        };

                        let channel = match buffers.get(&event.channel_id) {
                            Some(channel) => channel,
                            None => continue,
                        };

                        channel.remove_message(&conn.cache, event.id).await;
                    }
                },
                PluginMessage::MessageUpdate { message } => {
                    if let Some(guild_channel) = conn
                        .cache
                        .guild_channel(message.channel_id)
                        .await
                        .expect("InMemoryCache cannot fail")
                    {
                        let buffers = {
                            let guilds = session.guilds.borrow();
                            match guilds.get(&guild_channel.guild_id()) {
                                Some(guild) => guild.channel_buffers(),
                                None => continue,
                            }
                        };

                        let channel = match buffers.get(&message.channel_id) {
                            Some(channel) => channel,
                            None => continue,
                        };

                        channel.update_message(&conn.cache, *message).await;
                    }
                },
                PluginMessage::MemberChunk(member_chunk) => {
                    let channel_id = member_chunk
                        .nonce
                        .and_then(|id| id.parse().ok().map(ChannelId));
                    if let Some(channel_id) = channel_id {
                        if let Some(guild_channel) = conn
                            .cache
                            .guild_channel(channel_id)
                            .await
                            .expect("InMemoryCache cannot fail")
                        {
                            let buffers = {
                                let guilds = session.guilds.borrow();
                                match guilds.get(&guild_channel.guild_id()) {
                                    Some(guild) => guild.channel_buffers(),
                                    None => continue,
                                }
                            };

                            let channel = match buffers.get(&channel_id) {
                                Some(channel) => channel,
                                None => continue,
                            };

                            channel.redraw(&conn.cache).await;
                        }
                    }
                },
            }
        }
    }

    async fn handle_gateway_event(event: GatewayEvent, mut tx: Sender<PluginMessage>) {
        match event {
            GatewayEvent::Ready(ready) => {
                tx.send(PluginMessage::Connected { user: ready.user })
                    .await
                    .ok()
                    .unwrap();
            },
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
                        event: twilight::model::gateway::payload::MessageDelete {
                            channel_id: event.channel_id,
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
