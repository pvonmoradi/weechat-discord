use crate::{
    discord::discord_connection::DiscordConnection,
    guild_buffer::DiscordGuild,
    refcell::{Ref, RefCell, RefMut},
};
use std::{
    cell::BorrowMutError,
    collections::HashMap,
    rc::Rc,
    result::Result as StdResult,
    sync::atomic::{AtomicBool, Ordering},
};
use tokio::sync::mpsc::channel;
use tracing::*;
use tracing_subscriber::{filter::LevelFilter, EnvFilter};
use twilight::model::id::GuildId;
use weechat::{weechat_plugin, Args, Plugin, Weechat};

pub static ALIVE: Liveness = Liveness::new();

mod channel_buffer;
mod config;
mod debug;
mod discord;
mod format;
mod guild_buffer;
mod hooks;
mod message_renderer;
mod refcell;
mod twilight_utils;
mod utils;

#[derive(Clone)]
pub struct Guilds {
    guilds: Rc<RefCell<HashMap<GuildId, DiscordGuild>>>,
}

impl Guilds {
    pub fn borrow(&self) -> Ref<'_, HashMap<GuildId, DiscordGuild>> {
        self.guilds.borrow()
    }

    pub fn try_borrow_mut(
        &self,
    ) -> Result<RefMut<'_, HashMap<GuildId, DiscordGuild>>, BorrowMutError> {
        self.guilds.try_borrow_mut()
    }

    pub fn borrow_mut(&self) -> RefMut<'_, HashMap<GuildId, DiscordGuild>> {
        self.guilds.borrow_mut()
    }
}

impl Guilds {
    pub fn new() -> Guilds {
        Guilds {
            guilds: Rc::new(RefCell::new(HashMap::new())),
        }
    }
}

#[derive(Clone)]
pub struct DiscordSession {
    guilds: Guilds,
}

impl DiscordSession {
    pub fn new() -> DiscordSession {
        DiscordSession {
            guilds: Guilds::new(),
        }
    }
}

pub struct Weecord {
    _discord: DiscordSession,
    _discord_connection: DiscordConnection,
    _config: config::Config,
    _hooks: hooks::Hooks,
}

impl Plugin for Weecord {
    fn init(weechat: &Weechat, _args: Args) -> StdResult<Self, ()> {
        let session = DiscordSession::new();

        let config = config::Config::new(&session);

        if config.read().is_err() {
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

        let discord_connection = DiscordConnection::new();

        if let Some(token) = config.token() {
            let (tx, rx) = channel(1000);

            let discord_connection = discord_connection.clone();
            let session = session.clone();
            Weechat::spawn(async move {
                if let Ok(connection) = discord_connection.start(&token, tx).await {
                    DiscordConnection::handle_events(rx, session, &connection).await;
                }
            });
        };

        let _hooks = hooks::Hooks::hook_all(
            weechat,
            discord_connection.clone(),
            session.clone(),
            config.clone(),
        );

        Ok(Weecord {
            _discord: session,
            _discord_connection: discord_connection,
            _config: config,
            _hooks,
        })
    }
}

impl Drop for Weecord {
    fn drop(&mut self) {
        ALIVE.set_dead();
        trace!("Plugin unloaded");
    }
}

pub struct Liveness {
    state: AtomicBool,
}

impl Liveness {
    pub const fn new() -> Liveness {
        Liveness {
            state: AtomicBool::new(true),
        }
    }

    pub fn alive(&self) -> bool {
        self.state.load(Ordering::Relaxed)
    }

    pub fn set_dead(&self) {
        self.state.store(false, Ordering::Relaxed);
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
