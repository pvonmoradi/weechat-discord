use crate::{config::Config, discord::discord_connection::DiscordConnection, DiscordSession};
use tracing::trace;
use weechat::{hooks::Command, Weechat};

pub mod command;
pub mod completions;
pub mod options;

pub struct Hooks {
    _completions: completions::Completions,
    _command: Command,
    _options: options::Options,
}

impl Hooks {
    pub fn hook_all(
        weechat: &Weechat,
        discord_connection: DiscordConnection,
        session: DiscordSession,
        config: Config,
    ) -> Hooks {
        let _command = command::hook(discord_connection.clone(), session, config.clone());
        trace!("Command hooked");

        let _completions = completions::Completions::hook_all(discord_connection);
        trace!("Completions hooked");

        let _options = options::Options::hook_all(weechat, config);
        trace!("Options hooked");

        Hooks {
            _completions,
            _command,
            _options,
        }
    }
}
