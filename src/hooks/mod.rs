use crate::{config::Config, discord::discord_connection::DiscordConnection, instance::Instance};
use weechat::Weechat;

mod command;
mod completions;
mod options;
pub use completions::Completions;
pub use options::Options;
pub use weechat::hooks::Command;

pub struct Hooks {
    _completions: completions::Completions,
    _command: Command,
    _options: options::Options,
}

impl Hooks {
    pub fn hook_all(
        weechat: &Weechat,
        discord_connection: DiscordConnection,
        instance: Instance,
        config: Config,
    ) -> Hooks {
        let _command = command::hook(discord_connection.clone(), instance, config.clone());
        tracing::trace!("Command hooked");

        let _completions = completions::Completions::hook_all(discord_connection);
        tracing::trace!("Completions hooked");

        let _options = options::Options::hook_all(weechat, config);
        tracing::trace!("Options hooked");

        Hooks {
            _completions,
            _command,
            _options,
        }
    }
}
