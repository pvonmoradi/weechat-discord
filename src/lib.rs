#![allow(clippy::let_unit_value)]

mod bar_items;
mod buffers;
mod command;
mod config;
mod discord;
mod hook;
mod sync;
mod utils;
mod weechat_utils;

use crate::weechat_utils::BufferManager;
pub use sync::{on_main, on_main_blocking, upgrade_plugin};

use std::borrow::Cow;
use weechat::{weechat_plugin, ArgsWeechat, ConfigOption, Weechat, WeechatPlugin, WeechatResult};

pub struct Discord {
    weechat: Weechat,
    config: config::Config,
    buffer_manager: BufferManager,
    _sync_handle: sync::SyncHandle,
    _hook_handles: hook::HookHandles,
    _bar_handles: bar_items::BarHandles,
}

impl WeechatPlugin for Discord {
    // Note: We cannot use on_main (or plugin_print)
    fn init(weechat: Weechat, args: ArgsWeechat) -> WeechatResult<Self> {
        let args: Vec<_> = args.collect();

        let _sync_handle = sync::init(&weechat);
        let _hook_handles = hook::init(&weechat);
        let _bar_handles = bar_items::init(&weechat);
        let config = config::init(&weechat);
        let buffer_manager = buffers::init(&weechat);

        let autostart = config.autostart.value();

        let weecord = Discord {
            weechat,
            config,
            buffer_manager,
            _sync_handle,
            _hook_handles,
            _bar_handles,
        };

        if !args.contains(&"-a".to_owned()) && autostart {
            weecord.connect();
        }

        Ok(weecord)
    }
}

impl Discord {
    fn connect(&self) {
        if crate::discord::DISCORD.lock().is_some() {
            plugin_print("Already connected");
            return;
        }

        let token = self.config.token.value().into_owned();

        let token = if token.starts_with("${sec.data") {
            self.eval_string_expression(&token).map(Cow::into_owned)
        } else {
            Some(token)
        };
        if let Some(t) = token {
            if !t.is_empty() {
                discord::init(&self, &t, self.config.irc_mode.value());
            } else {
                self.print("Error: weecord.main.token is not set. To set it, run:");
                self.print("/discord token 123456789ABCDEF");
            }
        } else {
            self.print("Error: failed to evaluate token option, expected valid ${sec.data...}");
        }
    }
}

impl Drop for Discord {
    fn drop(&mut self) {
        // TODO: Why is the config file not saved on quit?
        self.config.config.write()
    }
}

impl std::ops::Deref for Discord {
    type Target = Weechat;

    fn deref(&self) -> &Self::Target {
        &self.weechat
    }
}

weechat_plugin!(
    Discord,
    name: "weecord",
    author: "Noskcaj19",
    description: "Discord integration for weechat",
    version: "0.2.0",
    license: "MIT"
);

pub fn plugin_print(msg: &str) {
    let msg = msg.to_owned();
    on_main(move |weechat| weechat.print(&format!("discord: {}", msg)))
}
