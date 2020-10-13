use crate::{discord::discord_connection::DiscordConnection, instance::Instance, utils::Flag};
pub use refcell::RefCell;
use std::result::Result as StdResult;
use tokio::sync::mpsc::channel;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;
use weechat::{plugin, Args, Plugin, Weechat};
pub use weechat2::Weechat2;

mod buffer;
mod config;
mod debug;
mod discord;
mod hooks;
mod instance;
mod message_renderer;
mod nicklist;
mod refcell;
mod twilight_utils;
mod utils;
mod weechat2;

pub static SHUTTING_DOWN: Flag = Flag::new();

pub struct Weecord {
    _discord_connection: DiscordConnection,
    _config: config::Config,
    instance: Instance,
    _hooks: hooks::Hooks,
}

impl Plugin for Weecord {
    fn init(weechat: &Weechat, _: Args) -> StdResult<Self, ()> {
        let config = config::Config::new();

        if config.read(&config.config.borrow()).is_err() {
            return Err(());
        }

        let _ = tracing_subscriber::fmt()
            .with_env_filter(
                EnvFilter::new(config.log_directive())
                    // Set the default log level to warn
                    .add_directive(LevelFilter::WARN.into()),
            )
            .with_writer(move || debug::Debug)
            .without_time()
            .try_init();

        if config.auto_open_tracing() {
            let _ = debug::Debug::create_buffer();
        }

        let instance = Instance::new();

        let discord_connection = DiscordConnection::new();

        if let Some(token) = config.token() {
            let (tx, rx) = channel(1000);

            let discord_connection = discord_connection.clone();
            Weechat::spawn({
                let config = config.clone();
                let instance = instance.clone();
                async move {
                    if let Ok(connection) = discord_connection.start(&token, tx).await {
                        DiscordConnection::handle_events(rx, &connection, config, instance).await;
                    }
                }
            });
        };

        let _hooks = hooks::Hooks::hook_all(
            weechat,
            discord_connection.clone(),
            instance.clone(),
            config.clone(),
        );

        Ok(Weecord {
            _discord_connection: discord_connection,
            _config: config,
            instance,
            _hooks,
        })
    }
}

impl Drop for Weecord {
    fn drop(&mut self) {
        // Ensure all buffers are cleared
        self.instance.borrow_guilds_mut().clear();
        SHUTTING_DOWN.trigger();
        tracing::trace!("Plugin unloaded");
    }
}

plugin!(
    Weecord,
    name: "weecord",
    author: "Noskcaj19",
    description: "Discord integration for weechat",
    version: "0.3.0",
    license: "MIT"
);
