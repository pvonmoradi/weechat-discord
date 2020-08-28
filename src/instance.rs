use crate::{
    guild::Guild,
    refcell::{Ref, RefCell, RefMut},
};
use std::{cell::BorrowMutError, collections::HashMap, rc::Rc};
use twilight::model::id::GuildId;

#[derive(Clone)]
pub struct Instance {
    guilds: Rc<RefCell<HashMap<GuildId, Guild>>>,
}

impl Instance {
    pub fn new() -> Self {
        Self {
            guilds: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    pub fn borrow(&self) -> Ref<'_, HashMap<GuildId, Guild>> {
        self.guilds.borrow()
    }

    pub fn try_borrow_mut(&self) -> Result<RefMut<'_, HashMap<GuildId, Guild>>, BorrowMutError> {
        self.guilds.try_borrow_mut()
    }

    pub fn borrow_mut(&self) -> RefMut<'_, HashMap<GuildId, Guild>> {
        self.guilds.borrow_mut()
    }
}
