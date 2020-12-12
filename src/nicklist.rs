use crate::{
    discord::discord_connection::ConnectionInner,
    twilight_utils::{ext::MemberExt, Color},
    utils::color::colorize_string,
};
use std::{rc::Rc, sync::Arc};
use twilight_cache_inmemory::model::CachedMember;
use weechat::buffer::{BufferHandle, NickSettings};

pub struct Nicklist {
    conn: ConnectionInner,
    handle: Rc<BufferHandle>,
}

impl Nicklist {
    pub fn new(conn: &ConnectionInner, handle: Rc<BufferHandle>) -> Nicklist {
        Nicklist {
            conn: conn.clone(),
            handle,
        }
    }

    pub fn add_members(&self, members: &[Arc<CachedMember>]) {
        if let Ok(buffer) = self.handle.upgrade() {
            for member in members {
                let member_color = member
                    .color(&self.conn.cache)
                    .map(|c| c.as_8bit())
                    .unwrap_or_default()
                    .to_string();
                let member_display_name = colorize_string(&member.display_name(), &member_color);
                if let Some(role) = member.highest_role_info(&self.conn.cache) {
                    let role_color = Color::new(role.color).as_8bit().to_string();
                    if let Some(group) = buffer.search_nicklist_group(&role.name) {
                        if group
                            .add_nick(NickSettings::new(&member_display_name))
                            .is_err()
                        {
                            tracing::error!(user.id=?member.user.id, group=%role.name, "Unable to add nick to nicklist");
                        }
                    } else {
                        if let Ok(group) =
                            buffer.add_nicklist_group(&role.name, &role_color, true, None)
                        {
                            if group
                                .add_nick(NickSettings::new(&member_display_name))
                                .is_err()
                            {
                                tracing::error!(user.id=?member.user.id, group=%role.name, "Unable to add nick to nicklist");
                            }
                        } else {
                            if buffer
                                .add_nick(NickSettings::new(&member_display_name))
                                .is_err()
                            {
                                tracing::error!(user.id=?member.user.id, "Unable to add nick to nicklist");
                            }
                        }
                    }
                } else {
                    if buffer
                        .add_nick(NickSettings::new(&member_display_name))
                        .is_err()
                    {
                        tracing::error!(user.id=?member.user.id, "Unable to add nick to nicklist");
                    }
                }
            }
        }
    }
}
