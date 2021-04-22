use crate::twilight_utils::ext::ChannelExt;
use std::sync::Arc;
use twilight_cache_inmemory::{model::CachedMember, InMemoryCache as Cache, InMemoryCache};
use twilight_model::{
    channel::{permission_overwrite::PermissionOverwrite, ChannelType, GuildChannel},
    guild::Permissions,
    id::{MessageId, RoleId, UserId},
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
    fn is_text_channel(&self, cache: &Cache) -> bool;
    fn last_message_id(&self) -> Option<MessageId>;
}

impl GuildChannelExt for GuildChannel {
    fn permission_overwrites(&self) -> &[PermissionOverwrite] {
        match self {
            GuildChannel::Category(c) => c.permission_overwrites.as_slice(),
            GuildChannel::Text(c) => c.permission_overwrites.as_slice(),
            GuildChannel::Voice(c) => c.permission_overwrites.as_slice(),
            GuildChannel::Stage(c) => c.permission_overwrites.as_slice(),
        }
    }

    fn topic(&self) -> Option<String> {
        match self {
            GuildChannel::Text(c) => c.topic.clone(),
            GuildChannel::Category(_) | GuildChannel::Voice(_) | GuildChannel::Stage(_) => None,
        }
    }

    fn members(&self, cache: &Cache) -> Result<Vec<Arc<CachedMember>>, ()> {
        match self {
            GuildChannel::Category(_) | GuildChannel::Voice(_) | GuildChannel::Stage(_) => Err(()),
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

    fn is_text_channel(&self, cache: &InMemoryCache) -> bool {
        if !self
            .has_permission(
                cache,
                Permissions::READ_MESSAGE_HISTORY | Permissions::VIEW_CHANNEL,
            )
            .unwrap_or(false)
        {
            return false;
        }

        match self {
            GuildChannel::Category(c) => c.kind == ChannelType::GuildText,
            GuildChannel::Text(c) => c.kind == ChannelType::GuildText,
            GuildChannel::Voice(_) | GuildChannel::Stage(_) => false,
        }
    }

    fn last_message_id(&self) -> Option<MessageId> {
        match self {
            GuildChannel::Text(c) => c.last_message_id,
            GuildChannel::Category(_) | GuildChannel::Voice(_) | GuildChannel::Stage(_) => None,
        }
    }
}
