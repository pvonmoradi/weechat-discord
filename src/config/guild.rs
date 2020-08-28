use crate::{
    config::Config,
    refcell::{RefCell, RefMut},
};
use std::rc::{Rc, Weak};
use twilight::model::id::{ChannelId, GuildId};
use weechat::{
    config::{
        BaseConfigOption, BooleanOptionSettings, ConfigSection, StringOption, StringOptionSettings,
    },
    Weechat,
};

#[derive(Clone, Debug)]
pub struct GuildConfigInner {
    autoconnect: bool,
    autojoin: Vec<ChannelId>,
}

impl GuildConfigInner {
    pub fn new() -> Self {
        GuildConfigInner {
            autoconnect: false,
            autojoin: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct GuildConfig {
    inner: Rc<RefCell<GuildConfigInner>>,
    id: GuildId,
}

impl GuildConfig {
    pub fn new(guild_section: &mut ConfigSection, id: GuildId) -> Self {
        let inner = Rc::new(RefCell::new(GuildConfigInner::new()));

        let weak_inner = Rc::downgrade(&inner);

        let inner_clone = Weak::clone(&weak_inner);
        let autoconnect = BooleanOptionSettings::new(format!("{}.autoconnect", id.0))
            .set_change_callback(move |_, option| {
                let inner = inner_clone.upgrade().expect("Config has outlived guild");

                inner.borrow_mut().autoconnect = option.value();
            });

        guild_section
            .new_boolean_option(autoconnect)
            .expect("Unable to create autoconnect option");

        let inner_clone = Weak::clone(&weak_inner);
        let autojoin_channels = StringOptionSettings::new(format!("{}.autojoin", id.0))
            .set_check_callback(|_: &Weechat, _: &StringOption, value| {
                if value.is_empty() {
                    true
                } else {
                    value.split(',').all(|ch| ch.parse::<u64>().is_ok())
                }
            })
            .set_change_callback(move |_, option| {
                let inner = inner_clone.upgrade().expect("Config has outlived guild");

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

                inner.borrow_mut().autojoin = channels;
            });
        guild_section
            .new_string_option(autojoin_channels)
            .expect("Unable to create autojoin channels option");

        GuildConfig { inner, id }
    }

    pub fn autoconnect(&self) -> bool {
        self.inner.borrow().autoconnect
    }

    pub fn set_autoconnect(&self, autoconnect: bool) {
        self.inner.borrow_mut().autoconnect = autoconnect;
    }

    pub fn autojoin_channels(&self) -> Vec<ChannelId> {
        self.inner.borrow().autojoin.clone()
    }

    pub fn autojoin_channels_mut(&self) -> RefMut<Vec<ChannelId>> {
        RefMut::map(self.inner.borrow_mut(), |i| &mut i.autojoin)
    }

    pub fn write(&self, config: &Config) {
        let config = config.config.borrow();
        let section = config
            .search_section("server")
            .expect("Unable to get server section");

        let autojoin = section
            .search_option(&format!("{}.autojoin", self.id))
            .expect("autojoin option does not exist");
        autojoin.set(
            &self
                .autojoin_channels()
                .iter()
                .map(|c| c.0.to_string())
                .collect::<Vec<_>>()
                .join(","),
            true,
        );
    }
}
