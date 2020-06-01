use crate::discord::{event_handler::Handler, plugin_message::PluginMessage};
use tokio::{
    runtime::Runtime,
    sync::mpsc::{Receiver, Sender},
};
use tracing::*;
use weechat::Weechat;

pub struct DiscordConnection {
    _rt: Runtime,
}

impl DiscordConnection {
    pub fn start(token: &str, tx: Sender<PluginMessage>) -> DiscordConnection {
        let runtime = Runtime::new().expect("Unable to create tokio runtime");
        let token = token.to_owned();
        {
            let mut tx = tx.clone();
            runtime.spawn(async move {
                let mut client = match serenity::Client::new(&token)
                    .event_handler(Handler::new(tx.clone()))
                    .await
                {
                    Ok(client) => {
                        info!("Connected to Discord");
                        client
                    },
                    Err(error) => {
                        error!("An error occurred connecting to Discord: {:?}", error);
                        let _ = tx
                            .send(PluginMessage::SerenityError {
                                message: "An error occurred connecting to discord:".into(),
                                error,
                            })
                            .await;
                        return;
                    },
                };

                if let Err(e) = client.start().await {
                    error!("An error occurred with the Discord client: {:?}", e);
                }
            });
        }
        DiscordConnection { _rt: runtime }
    }

    pub async fn handle_events(mut rx: Receiver<PluginMessage>) {
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
                    Weechat::print(&format!("ready as: {}", user.name));
                },
                PluginMessage::SerenityError { message, error } => {
                    Weechat::print(&format!("{}: {}", message, error));
                },
            }
        }
    }
}
