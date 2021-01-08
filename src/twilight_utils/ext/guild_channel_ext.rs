use crate::twilight_utils::ext::{CachedGuildExt, ChannelExt};
use std::sync::Arc;
use twilight_cache_inmemory::{model::CachedMember, InMemoryCache as Cache};
use twilight_model::{
    channel::{permission_overwrite::PermissionOverwrite, GuildChannel},
    guild::Permissions,
    id::RoleId,
};

pub trait GuildChannelExt {
    fn permission_overwrites(&self) -> &[PermissionOverwrite];
    fn topic(&self) -> Option<String>;
    fn members(&self, cache: &Cache) -> Result<Vec<Arc<CachedMember>>, ()>;
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
                let guild = cache.guild(channel.guild_id.ok_or(())?).ok_or(())?;

                let members = cache.members(channel.guild_id.ok_or(())?).ok_or(())?;

                Ok(members
                    .iter()
                    .filter_map(|member| {
                        let guild = Arc::clone(&guild);
                        if guild
                            .permissions_in(cache, channel.id, member.user.id)
                            .contains(Permissions::READ_MESSAGE_HISTORY)
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

    fn has_permission(&self, cache: &Cache, permissions: Permissions) -> Option<bool> {
        let current_user = cache.current_user()?;

        let guild_id = self.guild_id().expect("guild channel must have a guild id");
        let member = cache.member(guild_id, current_user.id)?;

        let roles: Vec<_> = member
            .roles
            .iter()
            .chain(Some(&RoleId(guild_id.0)))
            .flat_map(|&role_id| cache.role(role_id))
            .map(|role| (role.id, role.permissions))
            .collect();

        let calc =
            twilight_permission_calculator::Calculator::new(guild_id, current_user.id, &roles);
        let perms = calc.in_channel(self.kind(), self.permission_overwrites());

        if let Ok(perms) = perms {
            if perms.contains(permissions) {
                return Some(true);
            }
        }
        Some(false)
    }
}
