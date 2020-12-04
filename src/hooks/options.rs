use crate::config::Config;
use weechat::{config::ConfigOption, Weechat};

pub struct Options {}

impl Options {
    pub fn hook_all(weechat: &Weechat, config: Config) -> Options {
        // TODO: Use hook_config to hook these
        if let ConfigOption::String(option) = weechat
            .config_get("weechat.look.nick_prefix")
            .expect("Builtin option weechat.look.nick_prefix must exist")
        {
            config.borrow_inner_mut().look.nick_prefix = option.value().to_string();
        }
        if let ConfigOption::String(option) = weechat
            .config_get("weechat.look.nick_suffix")
            .expect("Builtin option weechat.look.nick_suffix must exist")
        {
            config.borrow_inner_mut().look.nick_suffix = option.value().to_string();
        }
        if let ConfigOption::Color(option) = weechat
            .config_get("weechat.color.chat_nick_prefix")
            .expect("Builtin option weechat.color.chat_nick_prefix must exist")
        {
            config.borrow_inner_mut().color.nick_prefix_color = option.value().to_string();
        }
        if let ConfigOption::Color(option) = weechat
            .config_get("weechat.color.chat_nick_suffix")
            .expect("Builtin option weechat.color.chat_nick_suffix must exist")
        {
            config.borrow_inner_mut().color.nick_suffix_color = option.value().to_string();
        }

        Options {}
    }
}
