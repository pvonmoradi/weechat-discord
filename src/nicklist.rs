use crate::{discord::discord_connection::ConnectionMeta, twilight_utils::ext::MemberExt};
use std::{rc::Rc, sync::Arc};
use twilight::cache::twilight_cache_inmemory::model::CachedMember;
use weechat::buffer::{BufferHandle, NickSettings};

pub struct Nicklist {
    conn: ConnectionMeta,
    handle: Rc<BufferHandle>,
}

impl Nicklist {
    pub fn new(conn: &ConnectionMeta, handle: Rc<BufferHandle>) -> Nicklist {
        Nicklist {
            conn: conn.clone(),
            handle,
        }
    }

    pub async fn add_members(&self, members: &[Arc<CachedMember>]) {
        if let Ok(buffer) = self.handle.upgrade() {
            for member in members {
                if let Some(role) = member.highest_role_info(&self.conn.cache).await {
                    if let Ok(group) = buffer.add_nicklist_group(&role.name, "", true, None) {
                        if let Err(_) = group.add_nick(NickSettings::new(&member.display_name())) {
                            tracing::error!(user.id=?member.user.id, group=%role.name, "Unable to add nick to nicklist");
                        }
                    } else {
                        if let Err(_) = buffer.add_nick(NickSettings::new(&member.display_name())) {
                            tracing::error!(user.id=?member.user.id, "Unable to add nick to nicklist");
                        }
                    }
                }
            }
        }
    }
}
