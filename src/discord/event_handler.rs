use crate::discord::plugin_message::PluginMessage;
use serenity::{model::prelude::*, prelude::*};
use tokio::sync::mpsc::Sender;
use tracing::*;
use weechat::Weechat;

pub struct Handler {
    tx: Sender<PluginMessage>,
}

impl Handler {
    pub fn new(tx: Sender<PluginMessage>) -> Handler {
        Handler { tx }
    }
}

#[serenity::async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _ctx: Context, ready: Ready) {
        info!(
            "Discord ready as {}#{:04}",
            ready.user.name, ready.user.discriminator
        );
        let mut tx = self.tx.clone();
        Weechat::spawn_from_thread(async move {
            let _ = tx.send(PluginMessage::Connected { user: ready.user }).await;
        });
    }
}
