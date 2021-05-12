use crate::twilight_utils::ext::ChannelExt;
use std::{collections::HashMap, sync::Arc};
use twilight_cache_inmemory::InMemoryCache;
use twilight_model::{
    gateway::payload::{
        GroupId, MemberListId, MemberListItem, MemberListUpdate, MemberListUpdateOp,
    },
    guild::Role,
    id::ChannelId,
};

pub trait GroupIdExt {
    fn role(&self, cache: &InMemoryCache) -> Option<Arc<Role>>;
    fn name(&self, cache: &InMemoryCache) -> Option<String>;
}

impl GroupIdExt for GroupId {
    fn role(&self, cache: &InMemoryCache) -> Option<Arc<Role>> {
        match self {
            GroupId::Online => None,
            GroupId::Offline => None,
            GroupId::RoleId(role_id) => cache.role(*role_id),
        }
    }

    fn name(&self, cache: &InMemoryCache) -> Option<String> {
        match self {
            GroupId::Online => Some("Online".into()),
            GroupId::Offline => Some("Offline".into()),
            GroupId::RoleId(role_id) => cache.role(*role_id).map(|role| role.name.clone()),
        }
    }
}

// Stores member lists for an entire guild
#[derive(Default)]
pub struct MemberList {
    /// Tracks raw member lists from discord
    pub raw_lists: HashMap<MemberListId, Vec<MemberListItem>>,
}

/// # Sync
/// A set of members (n <= 100) and position in the list.  Replace range with incoming items.
/// ## Note
/// Range is always 100, but may contain less than 100 items, in such a case, existing items should not be
/// removed.
/// # Insert
/// Inserts a single item at a given index. (Index seems to always be <= length of the list)
/// # Update
/// Replaces an item at a given index with a new item (seems to preserve type). Single item form of Sync
/// # Delete
/// Removes an item at a given index.  May be sent in conjunction with Inserts to move members/groups.
/// Single item form of Invalidate
/// # Invalidate
/// Remove a range of items.

impl MemberList {
    pub fn apply_update(&mut self, update: MemberListUpdate) {
        let this_list = self.raw_lists.entry(update.id).or_default();
        for op in update.ops {
            match op {
                MemberListUpdateOp::Sync { range, items } => {
                    tracing::trace!(
                        "sync: range: {:?}, items.len(): {} id: {:?}",
                        range,
                        items.len(),
                        update.id
                    );
                    let offset = range[0] as usize;
                    for (i, item) in items.into_iter().enumerate() {
                        match this_list.get_mut(i + offset) {
                            Some(old_item) => *old_item = item,
                            None => this_list.push(item),
                        }
                    }
                },
                MemberListUpdateOp::Update { index, item } => {
                    tracing::trace!("update: index: {}", index,);
                    this_list[index as usize] = item;
                },
                MemberListUpdateOp::Delete { index } => {
                    // TODO: This currently does not properly handle actual deletion of groups.
                    //       since the gateway sends a group delete followed by a insert in order
                    //       to move it's position, we can't trivially tell if it's being moved,
                    //       or actually deleted
                    tracing::trace!("delete: index: {}", index);
                    this_list.remove(index as usize);
                },
                MemberListUpdateOp::Insert { index, item } => {
                    tracing::trace!("insert: index: {}", index);
                    this_list.insert(index as usize, item);
                },
                MemberListUpdateOp::Invalidate { range } => {
                    tracing::trace!("invalidate: range: {:?}", range);
                    this_list.drain((range[0] as usize)..(range[0] as usize));
                },
                MemberListUpdateOp::Unknown => unimplemented!(),
            }
        }
    }

    pub fn get_list_for_channel(
        &self,
        channel_id: ChannelId,
        cache: &InMemoryCache,
    ) -> Option<&Vec<MemberListItem>> {
        cache
            .guild_channel(channel_id)
            .map(|ch| ch.member_list_id(cache))
            .and_then(|id| self.raw_lists.get(&id))
    }
}
