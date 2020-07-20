use crate::{
    refcell::{RefCell, RefMut},
    DiscordGuild, DiscordSession,
};
use anyhow::Result;
use std::rc::{Rc, Weak};
use tracing_subscriber::EnvFilter;
use twilight::model::id::GuildId;
use weechat::{
    config::{
        BooleanOptionSettings, Conf, Config as WeechatConfig, ConfigSection, ConfigSectionSettings,
        IntegerOptionSettings, OptionChanged, SectionReadCallback, StringOption,
        StringOptionSettings,
    },
    Weechat,
};

#[derive(Clone)]
pub struct Config {
    pub(crate) config: Rc<RefCell<weechat::config::Config>>,
    inner: Rc<RefCell<InnerConfig>>,
    session: DiscordSession,
}

impl Config {
    pub fn borrow_mut(&self) -> RefMut<'_, weechat::config::Config> {
        self.config.borrow_mut()
    }

    pub fn borrow_inner_mut(&self) -> RefMut<'_, InnerConfig> {
        self.inner.borrow_mut()
    }
}

impl SectionReadCallback for Config {
    fn callback(
        &mut self,
        _: &Weechat,
        _: &Conf,
        section: &mut ConfigSection,
        option_name: &str,
        option_value: &str,
    ) -> OptionChanged {
        let option_args: Vec<&str> = option_name.splitn(2, '.').collect();

        let guild_id = option_args[0];

        {
            let mut guilds_borrow = self.session.guilds.borrow_mut();

            if let Ok(guild_id) = guild_id.parse().map(GuildId) {
                if !guilds_borrow.contains_key(&guild_id) {
                    let guild = DiscordGuild::new(&self, guild_id, section);
                    guilds_borrow.insert(guild_id, guild);
                }
            }
        }

        let option = section.search_option(option_name);

        if let Some(o) = option {
            o.set(option_value, true)
        } else {
            OptionChanged::NotFound
        }
    }
}

#[derive(Clone)]
pub struct InnerConfig {
    pub token: Option<String>,
    pub log_directive: String,
    pub auto_open_tracing: bool,
    pub message_fetch_count: i32,
    pub nick_prefix: String,
    pub nick_prefix_color: String,
    pub nick_suffix: String,
    pub nick_suffix_color: String,
}

impl InnerConfig {
    pub fn new() -> InnerConfig {
        InnerConfig {
            token: None,
            log_directive: "".to_string(),
            auto_open_tracing: false,
            message_fetch_count: 50,
            nick_prefix: "".to_string(),
            nick_prefix_color: "".to_string(),
            nick_suffix: "".to_string(),
            nick_suffix_color: "".to_string(),
        }
    }
}

impl Config {
    pub fn new(session: &DiscordSession) -> Config {
        let config = Rc::new(RefCell::new(
            WeechatConfig::new("weecord").expect("Can't create new config"),
        ));
        let inner = Rc::new(RefCell::new(InnerConfig::new()));

        let config = Config {
            config,
            inner: Rc::clone(&inner),
            session: session.clone(),
        };

        {
            let inner = Rc::downgrade(&inner);
            let general_secion_option = ConfigSectionSettings::new("general");
            let mut config_borrow = config.config.borrow_mut();
            let mut sec = config_borrow
                .new_section(general_secion_option)
                .expect("Unable to create general section");

            let inner_clone = Weak::clone(&inner);
            sec.new_string_option(
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
            sec.new_integer_option(
                IntegerOptionSettings::new("message_fetch_count")
                    .description("number of messages to fetch when opening a buffer")
                    .default_value(50)
                    .min(0)
                    .max(100)
                    .set_change_callback(move |_, option| {
                        let inner = inner_clone
                            .upgrade()
                            .expect("Outer config has outlived inner config");
                        inner.borrow_mut().message_fetch_count = option.value();
                    }),
            )
            .expect("Unable to create message fetch count option");

            let inner_clone = Weak::clone(&inner);
            sec.new_string_option(
                StringOptionSettings::new("log_directive")
                    .description("Directive to configure plugin logging")
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
            sec.new_boolean_option(
                BooleanOptionSettings::new("open_tracing_window")
                    .description("Should the tracing window be opened automatically")
                    .set_change_callback(move |_, option| {
                        let inner = inner_clone
                            .upgrade()
                            .expect("Outer config has outlived inner config");
                        inner.borrow_mut().auto_open_tracing = option.value();
                    }),
            )
            .expect("Unable to create tracing window option");
        }

        {
            let server_section_options = ConfigSectionSettings::new("server")
                .set_read_callback(config.clone())
                .set_write_callback(|_: &Weechat, config: &Conf, section: &mut ConfigSection| {
                    config.write_section(section.name());
                    for option in section.options() {
                        config.write_option(option);
                    }
                });
            let mut config_borrow = config.config.borrow_mut();
            config_borrow
                .new_section(server_section_options)
                .expect("Unable to create server section");
        }

        config
    }

    pub fn read(&self) -> Result<()> {
        Ok(self.config.borrow().read()?)
    }

    pub fn write(&self) {
        let config = self.config.borrow();
        let general = config
            .search_section("general")
            .expect("general option section must exist");

        general
            .search_option("token")
            .expect("token option must exist")
            .set(&self.token().unwrap_or_default(), false);
    }

    pub fn auto_open_tracing(&self) -> bool {
        self.inner.borrow().auto_open_tracing
    }

    pub fn token(&self) -> Option<String> {
        self.inner.borrow().token.clone()
    }

    pub fn log_directive(&self) -> String {
        self.inner.borrow().log_directive.clone()
    }

    pub fn message_fetch_count(&self) -> i32 {
        self.inner.borrow().message_fetch_count
    }

    pub fn nick_prefix(&self) -> String {
        self.inner.borrow().nick_prefix.clone()
    }

    pub fn nick_prefix_color(&self) -> String {
        self.inner.borrow().nick_prefix_color.clone()
    }

    pub fn nick_suffix(&self) -> String {
        self.inner.borrow().nick_suffix.clone()
    }

    pub fn nick_suffix_color(&self) -> String {
        self.inner.borrow().nick_suffix_color.clone()
    }
}

impl Drop for Config {
    fn drop(&mut self) {
        self.config
            .borrow()
            .write()
            .expect("Unable to write config file");
    }
}
