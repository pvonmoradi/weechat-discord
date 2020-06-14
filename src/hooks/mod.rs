use crate::{config::Config, discord::discord_connection::DiscordConnection, DiscordSession};
use weechat::{hooks::Command, Weechat};

pub mod command;
pub mod completions;

pub struct Hooks {
    _completions: completions::Completions,
    _command: Command,
}

impl Hooks {
    pub fn hook_all(
        weechat: &Weechat,
        discord_connection: DiscordConnection,
        session: DiscordSession,
        config: Config,
    ) -> Hooks {
        let _command = command::hook(
            weechat,
            discord_connection.clone(),
            session.clone(),
            config.clone(),
        );
        tracing::trace!("Command hooked");

        let _completions = completions::Completions::hook_all(weechat, discord_connection.clone());
        tracing::trace!("Completions hooked");

        Hooks {
            _completions,
            _command,
        }
    }
}
