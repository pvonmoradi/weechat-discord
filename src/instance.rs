use crate::{
    channel::Channel,
    guild::Guild,
    refcell::{Ref, RefCell, RefMut},
};
use std::{cell::BorrowMutError, collections::HashMap, rc::Rc};
use twilight_model::id::{ChannelId, GuildId};

#[derive(Clone)]
pub struct Instance {
    guilds: Rc<RefCell<HashMap<GuildId, Guild>>>,
    private_channels: Rc<RefCell<HashMap<ChannelId, Channel>>>,
}

impl Instance {
    pub fn new() -> Self {
        Self {
            guilds: Rc::new(RefCell::new(HashMap::new())),
            private_channels: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    pub fn borrow_guilds(&self) -> Ref<'_, HashMap<GuildId, Guild>> {
        self.guilds.borrow()
    }

    pub fn try_borrow_guilds_mut(
        &self,
    ) -> Result<RefMut<'_, HashMap<GuildId, Guild>>, BorrowMutError> {
        self.guilds.try_borrow_mut()
    }

    pub fn borrow_guilds_mut(&self) -> RefMut<'_, HashMap<GuildId, Guild>> {
        self.guilds.borrow_mut()
    }

    pub fn borrow_private_channels(&self) -> Ref<'_, HashMap<ChannelId, Channel>> {
        self.private_channels.borrow()
    }

    pub fn borrow_private_channels_mut(&self) -> RefMut<'_, HashMap<ChannelId, Channel>> {
        self.private_channels.borrow_mut()
    }
}
