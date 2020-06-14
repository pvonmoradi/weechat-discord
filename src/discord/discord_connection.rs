use crate::{discord::plugin_message::PluginMessage, DiscordSession};
use anyhow::Result;
use std::{cell::RefCell, rc::Rc, sync::Arc};
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
};
use weechat::Weechat;

pub type DiscordConnection = Rc<RefCell<Option<RawDiscordConnection>>>;

pub struct RawDiscordConnection {
    pub(crate) rt: Runtime,
    pub(crate) cache: Arc<Cache>,
    pub(crate) http: HttpClient,
}

impl RawDiscordConnection {
    pub async fn start(token: &str, tx: Sender<PluginMessage>) -> Result<RawDiscordConnection> {
        let (cache_tx, cache_rx) = channel();
        let runtime = Runtime::new().expect("Unable to create tokio runtime");
        let token = token.to_owned();
        {
            let tx = tx.clone();
            runtime.spawn(async move {
                let config = ShardConfig::builder(&token).build();
                let shard = match Shard::new(config).await {
                    Ok(shard) => shard,
                    Err(e) => {
                        let err_msg = format!("An error occured connecting to Discord: {}", e);
                        Weechat::spawn_from_thread(async move { Weechat::print(&err_msg) });
                        error!("An error occured connecting to Discord: {:#?}", e);
                        return;
                    },
                };

                let cache = Arc::new(Cache::new());

                info!("Connected to Discord");
                let mut events = shard.events().await;

                let http = shard.config().http_client();
                cache_tx
                    .send((cache.clone(), http.clone()))
                    .ok()
                    .expect("Cache receiver closed before data could be sent");

                while let Some(event) = events.next().await {
                    cache
                        .update(&event)
                        .await
                        .expect("InMemoryCache cannot error");

                    tokio::spawn(Self::handle_gateway_event(
                        event,
                        cache.clone(),
                        http.clone(),
                        tx.clone(),
                    ));
                }
            });
        }

        let (cache, http) = cache_rx
            .await
            .map_err(|_| anyhow::anyhow!("The connection to discord failed"))?;
        Ok(RawDiscordConnection {
            rt: runtime,
            cache,
            http,
        })
    }

    pub async fn handle_events(
        mut rx: Receiver<PluginMessage>,
        session: DiscordSession,
        cache: Arc<Cache>,
        http: &HttpClient,
        rt: &Runtime,
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
                            if let Err(e) = guild.connect(&cache, &http, rt).await {
                                warn!(guild_id = guild_id.0, error=?e, "Error connecting guild");
                            }
                        }
                    }
                },
            }
        }
    }

    async fn handle_gateway_event(
        event: GatewayEvent,
        _cache: Arc<Cache>,
        _http: HttpClient,
        mut tx: Sender<PluginMessage>,
    ) {
        match event {
            GatewayEvent::Ready(ready) => {
                tx.send(PluginMessage::Connected { user: ready.user })
                    .await
                    .ok()
                    .unwrap();
            },
            _ => {},
        }
    }
}
