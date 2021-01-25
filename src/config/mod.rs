//! This module provides Config structs which are isolated from the other data structures to facilitate
//! better isolation
use crate::refcell::{RefCell, RefMut};
use anyhow::Result;
use std::{
    collections::HashMap,
    rc::{Rc, Weak},
};
use tracing_subscriber::EnvFilter;
use twilight_model::id::{ChannelId, GuildId};
use weechat::{
    config::{
        BooleanOptionSettings, Conf, Config as WeechatConfig, ConfigSection, ConfigSectionSettings,
        IntegerOptionSettings, OptionChanged, StringOption, StringOptionSettings,
    },
    Weechat,
};

mod guild;

pub use guild::{GuildConfig, GuildConfigInner};
use weechat::config::BaseConfigOption;

#[derive(Clone)]
pub struct Config {
    pub(crate) config: Rc<RefCell<weechat::config::Config>>,
    inner: Rc<RefCell<InnerConfig>>,
}

impl Config {
    pub fn borrow_inner_mut(&self) -> RefMut<'_, InnerConfig> {
        self.inner.borrow_mut()
    }
}

pub struct LookConfig {
    pub nick_prefix: String,
    pub nick_suffix: String,
    pub auto_open_tracing: bool,
    pub typing_list_style: i32,
    pub typing_list_max: i32,
    pub show_unknown_user_ids: bool,
    pub message_fetch_count: i32,
    pub readonly_value: String,
    pub image_max_height: i32,
}

impl Default for LookConfig {
    fn default() -> LookConfig {
        LookConfig {
            auto_open_tracing: false,
            show_unknown_user_ids: false,
            nick_prefix: "".to_string(),
            nick_suffix: "".to_string(),
            typing_list_max: 5,
            typing_list_style: 0,
            message_fetch_count: 50,
            readonly_value: "🔒".to_string(),
            image_max_height: 15,
        }
    }
}

pub struct ColorConfig {
    pub nick_prefix_color: String,
    pub nick_suffix_color: String,
}

impl Default for ColorConfig {
    fn default() -> ColorConfig {
        ColorConfig {
            nick_prefix_color: "".to_string(),
            nick_suffix_color: "".to_string(),
        }
    }
}

pub struct InnerConfig {
    pub look: LookConfig,
    pub color: ColorConfig,
    pub token: Option<String>,
    pub log_directive: String,
    pub guilds: HashMap<GuildId, GuildConfig>,
    pub autojoin_private: Vec<ChannelId>,
    // Should we use value of weechat.history.max_buffer_lines_number here instead?
    pub max_buffer_messages: i32,
    pub send_typing: bool,
}

impl Default for InnerConfig {
    fn default() -> InnerConfig {
        InnerConfig {
            look: LookConfig::default(),
            color: ColorConfig::default(),
            token: None,
            log_directive: "".to_string(),
            guilds: HashMap::new(),
            autojoin_private: Vec::new(),
            max_buffer_messages: 4096,
            send_typing: false,
        }
    }
}

impl Config {
    pub fn new() -> Config {
        let mut weechat_config = WeechatConfig::new("weecord").expect("Can't create new config");
        let inner = Rc::new(RefCell::new(InnerConfig::default()));

        {
            let inner = Rc::downgrade(&inner);
            let general_section_option = ConfigSectionSettings::new("general");
            let mut general = weechat_config
                .new_section(general_section_option)
                .expect("Unable to create general section");

            let inner_clone = Weak::clone(&inner);
            general
                .new_string_option(
                    StringOptionSettings::new("token")
                        .description("Discord auth token. Supports secure data")
                        .set_change_callback(move |_, option| {
                            let inner = inner_clone
                                .upgrade()
                                .expect("Outer config has outlived inner config");
                            inner.borrow_mut().token = Some(option.value().to_string());
                        }),
                )
                .expect("Unable to create token option");

            let inner_clone = Weak::clone(&inner);
            general
                .new_string_option(
                    StringOptionSettings::new("log_directive")
                        .description(
                            "tracing-style env-logger directive to configure plugin logging",
                        )
                        .default_value("weecord=warn")
                        .set_change_callback(move |_, option| {
                            let inner = inner_clone
                                .upgrade()
                                .expect("Outer config has outlived inner config");

                            inner.borrow_mut().log_directive = option.value().to_string();
                        })
                        .set_check_callback(|_: &Weechat, _: &StringOption, value| {
                            EnvFilter::try_new(value.as_ref()).is_ok()
                        }),
                )
                .expect("Unable to create tracing level option");

            let inner_clone = Weak::clone(&inner);
            general
                .new_string_option(
                    StringOptionSettings::new("autojoin_private")
                        .description("List of private channels to autojoin")
                        .set_change_callback(move |_, option| {
                            let inner = inner_clone
                                .upgrade()
                                .expect("Outer config has outlived inner config");

                            let mut channels: Vec<_> = option
                                .value()
                                .split(',')
                                .map(|ch| ch.parse().map(ChannelId))
                                .flatten()
                                .collect();

                            channels.sort();
                            channels.dedup();

                            option.set(
                                &channels
                                    .iter()
                                    .map(|c| c.0.to_string())
                                    .collect::<Vec<_>>()
                                    .join(","),
                                false,
                            );

                            inner.borrow_mut().autojoin_private = channels;
                        }),
                )
                .expect("Unable to create autojoin private option");

            let inner_clone = Weak::clone(&inner);
            general
                .new_integer_option(
                    IntegerOptionSettings::new("max_buffer_messages")
                        .description("maximum number of messages to store in the internal buffer")
                        .default_value(4096)
                        .max(i32::max_value())
                        .set_change_callback(move |_, option| {
                            let inner = inner_clone
                                .upgrade()
                                .expect("Outer config has outlived inner config");

                            inner.borrow_mut().max_buffer_messages = option.value();
                        }),
                )
                .expect("Unable to create max buffer messages option");

            let inner_clone = Weak::clone(&inner);
            general
                .new_boolean_option(
                    BooleanOptionSettings::new("send_typing")
                        .description("Should typing status be sent to discord")
                        .set_change_callback(move |_, option| {
                            let inner = inner_clone
                                .upgrade()
                                .expect("Outer config has outlived inner config");
                            inner.borrow_mut().send_typing = option.value();
                        }),
                )
                .expect("Unable to create send typing option");
        }

        {
            let inner = Rc::downgrade(&inner);
            let look_section_options = ConfigSectionSettings::new("look");
            let mut look = weechat_config
                .new_section(look_section_options)
                .expect("Unable to create look section");

            let inner_clone = Weak::clone(&inner);
            look.new_boolean_option(
                BooleanOptionSettings::new("open_tracing_window")
                    .description("Should the tracing window be opened automatically")
                    .set_change_callback(move |_, option| {
                        let inner = inner_clone
                            .upgrade()
                            .expect("Outer config has outlived inner config");
                        inner.borrow_mut().look.auto_open_tracing = option.value();
                    }),
            )
            .expect("Unable to create tracing window option");

            let inner_clone = Weak::clone(&inner);
            look.new_integer_option(
                IntegerOptionSettings::new("message_fetch_count")
                    .description("Number of messages to fetch when opening a buffer")
                    .default_value(50)
                    .min(0)
                    .max(100)
                    .set_change_callback(move |_, option| {
                        let inner = inner_clone
                            .upgrade()
                            .expect("Outer config has outlived inner config");
                        inner.borrow_mut().look.message_fetch_count = option.value();
                    }),
            )
            .expect("Unable to create message fetch count option");

            let inner_clone = Weak::clone(&inner);
            look.new_integer_option(
                IntegerOptionSettings::new("typing_list_max")
                    .description("Maximum number of users to display in the typing list")
                    .min(0)
                    .max(100)
                    .default_value(5)
                    .set_change_callback(move |_, option| {
                        let inner = inner_clone
                            .upgrade()
                            .expect("Outer config has outlived inner config");

                        inner.borrow_mut().look.typing_list_max = option.value();
                    }),
            )
            .expect("Unable to create typing list max option");

            let inner_clone = Weak::clone(&inner);
            look.new_integer_option(
                IntegerOptionSettings::new("typing_list_style")
                    .description("Style of the typing list")
                    .default_value(0)
                    .min(0)
                    .max(1)
                    .set_change_callback(move |_, option| {
                        let inner = inner_clone
                            .upgrade()
                            .expect("Outer config has outlived inner config");

                        inner.borrow_mut().look.typing_list_style = option.value();
                    }),
            )
            .expect("Unable to create typing list style option");

            let inner_clone = Weak::clone(&inner);
            look.new_boolean_option(
                BooleanOptionSettings::new("show_unknown_user_ids")
                    .description(
                        "Should unknown users be shown as @<user-id> instead of @unknown-user",
                    )
                    .set_change_callback(move |_, option| {
                        let inner = inner_clone
                            .upgrade()
                            .expect("Outer config has outlived inner config");
                        inner.borrow_mut().look.show_unknown_user_ids = option.value();
                    }),
            )
            .expect("Unable to create show unknown user ids option");

            let inner_clone = Weak::clone(&inner);
            look.new_string_option(
                StringOptionSettings::new("readonly_value")
                    .description("Value of the readonly bar item when a buffer is readonly")
                    .default_value("🔒")
                    .set_change_callback(move |_, option| {
                        let inner = inner_clone
                            .upgrade()
                            .expect("Outer config has outlived inner config");
                        inner.borrow_mut().look.readonly_value = option.value().to_string();
                    }),
            )
            .expect("Unable to create readonly value option");

            let inner_clone = Weak::clone(&inner);
            look.new_integer_option(
                IntegerOptionSettings::new("image_max_height")
                    .description("Maximum height for inline images")
                    .min(0)
                    .max(1000)
                    .default_value(40)
                    .set_change_callback(move |_, option| {
                        let inner = inner_clone
                            .upgrade()
                            .expect("Outer config has outlived inner config");
                        inner.borrow_mut().look.image_max_height = option.value();
                    }),
            )
            .expect("Unable to create image max height option");
        }

        {
            let inner = Rc::downgrade(&inner);
            let server_section_options = ConfigSectionSettings::new("server")
                .set_read_callback(
                    move |_: &Weechat,
                          _: &Conf,
                          section: &mut ConfigSection,
                          option_name: &str,
                          option_value: &str| {
                        let option_args: Vec<&str> = option_name.splitn(2, '.').collect();

                        let guild_id = option_args[0];

                        {
                            let inner = Weak::upgrade(&inner)
                                .expect("Outer config has outlived inner config");
                            let guilds = &mut inner.borrow_mut().guilds;

                            if let Ok(guild_id) = guild_id.parse().map(GuildId) {
                                guilds
                                    .entry(guild_id)
                                    .or_insert_with(|| GuildConfig::new(section, guild_id));
                            }
                        }

                        let option = section.search_option(option_name);

                        if let Some(o) = option {
                            o.set(option_value, true)
                        } else {
                            OptionChanged::NotFound
                        }
                    },
                )
                .set_write_callback(|_: &Weechat, config: &Conf, section: &mut ConfigSection| {
                    config.write_section(section.name());
                    for option in section.options() {
                        config.write_option(option);
                    }
                });
            weechat_config
                .new_section(server_section_options)
                .expect("Unable to create server section");
        }

        Config {
            config: Rc::new(RefCell::new(weechat_config)),
            inner: Rc::clone(&inner),
        }
    }

    pub fn read(&self, config: &weechat::config::Config) -> Result<()> {
        Ok(config.read()?)
    }

    pub fn auto_open_tracing(&self) -> bool {
        self.inner.borrow().look.auto_open_tracing
    }

    pub fn show_unknown_user_ids(&self) -> bool {
        self.inner.borrow().look.show_unknown_user_ids
    }

    pub fn token(&self) -> Option<String> {
        self.inner.borrow().token.clone()
    }

    pub fn log_directive(&self) -> String {
        self.inner.borrow().log_directive.clone()
    }

    pub fn message_fetch_count(&self) -> i32 {
        self.inner.borrow().look.message_fetch_count
    }

    pub fn send_typing(&self) -> bool {
        self.inner.borrow().send_typing
    }

    pub fn nick_prefix(&self) -> String {
        self.inner.borrow().look.nick_prefix.clone()
    }

    pub fn nick_prefix_color(&self) -> String {
        self.inner.borrow().color.nick_prefix_color.clone()
    }

    pub fn nick_suffix(&self) -> String {
        self.inner.borrow().look.nick_suffix.clone()
    }

    pub fn nick_suffix_color(&self) -> String {
        self.inner.borrow().color.nick_suffix_color.clone()
    }

    pub fn guilds(&self) -> HashMap<GuildId, GuildConfig> {
        self.inner.borrow().guilds.clone()
    }

    pub fn autojoin_private(&self) -> Vec<ChannelId> {
        self.inner.borrow().autojoin_private.clone()
    }

    pub fn typing_list_max(&self) -> i32 {
        self.inner.borrow().look.typing_list_max
    }

    pub fn max_buffer_messages(&self) -> i32 {
        self.inner.borrow().max_buffer_messages
    }

    pub fn typing_list_style(&self) -> i32 {
        self.inner.borrow().look.typing_list_style
    }

    pub fn image_max_height(&self) -> i32 {
        self.inner.borrow().look.image_max_height
    }

    pub fn persist(&self) {
        let config = self.config.borrow();
        let general = config
            .search_section("general")
            .expect("general option section must exist");

        general
            .search_option("token")
            .expect("token option must exist")
            .set(&self.token().unwrap_or_default(), false);

        general
            .search_option("log_directive")
            .expect("log directive option must exist")
            .set(&self.log_directive(), false);

        general
            .search_option("autojoin_private")
            .expect("autojoin private option must exist")
            .set(
                &self
                    .autojoin_private()
                    .iter()
                    .map(|c| c.0.to_string())
                    .collect::<Vec<_>>()
                    .join(","),
                false,
            );

        general
            .search_option("max_buffer_messages")
            .expect("max buffer messages option must exist")
            .set(&self.max_buffer_messages().to_string(), false);

        general
            .search_option("send_typing")
            .expect("send typing options must exist")
            .set(if self.send_typing() { "true" } else { "false" }, false);

        let look = config
            .search_section("look")
            .expect("look option section must exist");

        look.search_option("typing_list_max")
            .expect("typing list max option must exist")
            .set(&self.typing_list_max().to_string(), false);

        look.search_option("typing_list_style")
            .expect("typing list style option must exist")
            .set(&self.typing_list_style().to_string(), false);

        look.search_option("show_unknown_user_ids")
            .expect("show unknown user ids option must exist")
            .set(
                if self.show_unknown_user_ids() {
                    "true"
                } else {
                    "false"
                },
                false,
            );

        look.search_option("message_fetch_count")
            .expect("message fetch count option must exist")
            .set(&self.message_fetch_count().to_string(), false);

        look.search_option("open_tracing_window")
            .expect("log directive option must exist")
            .set(
                if self.auto_open_tracing() {
                    "true"
                } else {
                    "false"
                },
                false,
            );

        look.search_option("image_max_height")
            .expect("image max height option must exist")
            .set(&self.image_max_height().to_string(), false);
    }
}

impl Drop for Config {
    fn drop(&mut self) {
        self.persist();
        let _ = self.config.borrow().write();
    }
}
