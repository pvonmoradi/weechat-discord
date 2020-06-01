use crate::discord::discord_connection::DiscordConnection;
use std::result::Result as StdResult;
use tokio::sync::mpsc::channel;
use weechat::{weechat_plugin, ArgsWeechat, Weechat, WeechatPlugin};

mod config;
mod debug;
mod discord;

pub struct Weecord {
    _discord_connection: Option<DiscordConnection>,
    _config: config::Config,
}

impl WeechatPlugin for Weecord {
    fn init(_weechat: &Weechat, _args: ArgsWeechat) -> StdResult<Self, ()> {
        let config = config::Config::new();

        if let Err(_) = config.read() {
            return Err(());
        }

        tracing_subscriber::fmt()
            .with_writer(|| debug::Debug)
            .without_time()
            .with_max_level(config.tracing_level())
            .init();

        if config.auto_open_tracing() {
            let _ = debug::Debug::create_buffer();
        }

        let discord_connection = match config.token() {
            Some(token) => {
                let token = token.to_string();

                let (tx, rx) = channel(1000);

                let connection = DiscordConnection::start(&token, tx);

                Weechat::spawn(DiscordConnection::handle_events(rx));

                Some(connection)
            },
            None => None,
        };

        Ok(Weecord {
            _discord_connection: discord_connection,
            _config: config,
        })
    }
}

impl Drop for Weecord {
    fn drop(&mut self) {
        self._config
            .config
            .borrow()
            .write()
            .expect("Unable to write config file");
    }
}

weechat_plugin!(
    Weecord,
    name: "weecord",
    author: "Noskcaj19",
    description: "Discord integration for weechat",
    version: "0.3.0",
    license: "MIT"
);
