use crate::twilight_utils::ext::ChannelExt;
use std::sync::Arc;
use twilight_cache_inmemory::{model::CachedMember, InMemoryCache as Cache};
use twilight_model::{
    channel::{permission_overwrite::PermissionOverwrite, GuildChannel},
    guild::Permissions,
    id::{RoleId, UserId},
};

pub trait GuildChannelExt {
    fn permission_overwrites(&self) -> &[PermissionOverwrite];
    fn topic(&self) -> Option<String>;
    fn members(&self, cache: &Cache) -> Result<Vec<Arc<CachedMember>>, ()>;
    fn member_has_permission(
        &self,
        cache: &Cache,
        member: UserId,
        permissions: Permissions,
    ) -> Option<bool>;
    fn has_permission(&self, cache: &Cache, permissions: Permissions) -> Option<bool>;
}

impl GuildChannelExt for GuildChannel {
    fn permission_overwrites(&self) -> &[PermissionOverwrite] {
        match self {
            GuildChannel::Category(c) => c.permission_overwrites.as_slice(),
            GuildChannel::Text(c) => c.permission_overwrites.as_slice(),
            GuildChannel::Voice(c) => c.permission_overwrites.as_slice(),
        }
    }

    fn topic(&self) -> Option<String> {
        match self {
            GuildChannel::Category(_) => None,
            GuildChannel::Text(c) => c.topic.clone(),
            GuildChannel::Voice(_) => None,
        }
    }

    fn members(&self, cache: &Cache) -> Result<Vec<Arc<CachedMember>>, ()> {
        match self {
            GuildChannel::Category(_) => Err(()),
            GuildChannel::Voice(_) => Err(()),
            GuildChannel::Text(channel) => {
                let members = cache.members(channel.guild_id.ok_or(())?).ok_or(())?;

                Ok(members
                    .iter()
                    .filter_map(|member| {
                        if self
                            .member_has_permission(
                                cache,
                                member.user.id,
                                Permissions::READ_MESSAGE_HISTORY,
                            )
                            .unwrap_or(false)
                        {
                            Some(Arc::clone(member))
                        } else {
                            None
                        }
                    })
                    .collect())
            },
        }
    }

    fn member_has_permission(
        &self,
        cache: &Cache,
        member_id: UserId,
        permissions: Permissions,
    ) -> Option<bool> {
        let guild_id = self.guild_id().expect("guild channel must have a guild id");
        let member = cache.member(guild_id, member_id)?;

        let roles: Vec<_> = member
            .roles
            .iter()
            .chain(Some(&RoleId(guild_id.0)))
            .flat_map(|&role_id| cache.role(role_id))
            .map(|role| (role.id, role.permissions))
            .collect();

        let calc = twilight_permission_calculator::Calculator::new(guild_id, member_id, &roles);
        let perms = calc.in_channel(self.kind(), self.permission_overwrites());

        if let Ok(perms) = perms {
            if perms.contains(permissions) {
                return Some(true);
            }
        }
        Some(false)
    }

    fn has_permission(&self, cache: &Cache, permissions: Permissions) -> Option<bool> {
        let current_user = cache.current_user()?;

        self.member_has_permission(cache, current_user.id, permissions)
    }
}
