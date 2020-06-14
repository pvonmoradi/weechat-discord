use crate::{
    discord::discord_connection::{DiscordConnection, RawDiscordConnection},
    guild_buffer::DiscordGuild,
};
use std::{
    cell::{Ref, RefCell, RefMut},
    collections::HashMap,
    rc::Rc,
    result::Result as StdResult,
};
use tokio::sync::mpsc::channel;
use twilight::model::id::GuildId;
use weechat::{weechat_plugin, ArgsWeechat, Weechat, WeechatPlugin};

mod channel_buffer;
mod config;
mod debug;
mod discord;
mod guild_buffer;
mod hooks;
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

impl WeechatPlugin for Weecord {
    fn init(weechat: &Weechat, _args: ArgsWeechat) -> StdResult<Self, ()> {
        let session = DiscordSession::new();

        let config = config::Config::new(&session);

        if let Err(_) = config.read() {
            return Err(());
        }

        let _ = tracing_subscriber::fmt()
            .with_writer(|| debug::Debug)
            .without_time()
            .with_max_level(config.tracing_level())
            .try_init();

        if config.auto_open_tracing() {
            let _ = debug::Debug::create_buffer();
        }

        let discord_connection = Rc::new(RefCell::new(None));

        if let Some(token) = config.token() {
            let token = token.to_string();

            let (tx, rx) = channel(1000);

            let discord_connection = Rc::clone(&discord_connection);
            let session = session.clone();
            Weechat::spawn(async move {
                if let Ok(connection) = RawDiscordConnection::start(&token, tx).await {
                    let cache_clone = connection.cache.clone();
                    let http_clone = connection.http.clone();

                    discord_connection.borrow_mut().replace(connection);
                    RawDiscordConnection::handle_events(
                        rx,
                        session,
                        cache_clone,
                        &http_clone,
                        &discord_connection.borrow().as_ref().unwrap().rt,
                    )
                    .await;
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

weechat_plugin!(
    Weecord,
    name: "weecord",
    author: "Noskcaj19",
    description: "Discord integration for weechat",
    version: "0.3.0",
    license: "MIT"
);
