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

    pub fn try_borrow_private_channels_mut(
        &self,
    ) -> Result<RefMut<'_, HashMap<ChannelId, Channel>>, BorrowMutError> {
        self.private_channels.try_borrow_mut()
    }

    pub fn search_buffer(
        &self,
        guild_id: Option<GuildId>,
        channel_id: ChannelId,
    ) -> Option<Channel> {
        if let Some(guild_id) = guild_id {
            if let Some(guild) = self.guilds.borrow().get(&guild_id) {
                return guild.channels().get(&channel_id).cloned();
            }
        } else {
            return self.private_channels.borrow().get(&channel_id).cloned();
        }

        None
    }
}
