use crate::twilight_utils::ext::{ChannelExt, GuildChannelExt};
use twilight::{
    cache_inmemory::{model::CachedGuild, InMemoryCache as Cache},
    model::{
        channel::{permission_overwrite::PermissionOverwriteType, ChannelType},
        guild::Permissions,
        id::{ChannelId, RoleId, UserId},
    },
};

pub trait CachedGuildExt {
    fn permissions_in(&self, cache: &Cache, channel_id: ChannelId, user_id: UserId) -> Permissions;
}

impl CachedGuildExt for CachedGuild {
    fn permissions_in(&self, cache: &Cache, channel_id: ChannelId, user_id: UserId) -> Permissions {
        // The owner has all permissions in all cases.
        if user_id == self.owner_id {
            return Permissions::all();
        }

        // Start by retrieving the @everyone role's permissions.
        let everyone = match cache.role(RoleId(self.id.0)) {
            Some(everyone) => everyone,
            None => {
                tracing::error!("@everyone role ({}) missing in '{}'", self.id, self.name);

                return Permissions::empty();
            },
        };

        // Create a base set of permissions, starting with `@everyone`s.
        let mut permissions = everyone.permissions;

        let member = match cache.member(self.id, user_id) {
            Some(member) => member,
            None => return everyone.permissions,
        };

        for &role in &member.roles {
            if let Some(role) = cache.role(role) {
                permissions |= role.permissions;
            } else {
                tracing::warn!(
                    "{} on {} has non-existent role {:?}",
                    member.user.id,
                    self.id,
                    role
                );
            }
        }

        // Administrators have all permissions in any channel.
        if permissions.contains(Permissions::ADMINISTRATOR) {
            return Permissions::all();
        }

        if let Some(channel) = cache.guild_channel(channel_id) {
            // If this is a text channel, then throw out voice permissions.
            if channel.kind() == ChannelType::GuildText {
                permissions &= !(Permissions::CONNECT
                    | Permissions::SPEAK
                    | Permissions::MUTE_MEMBERS
                    | Permissions::DEAFEN_MEMBERS
                    | Permissions::MOVE_MEMBERS
                    | Permissions::USE_VAD);
            }

            // Apply the permission overwrites for the channel for each of the
            // overwrites that - first - applies to the member's roles, and then
            // the member itself.
            //
            // First apply the denied permission overwrites for each, then apply
            // the allowed.

            let mut data = Vec::with_capacity(member.roles.len());

            // Roles
            for overwrite in channel.permission_overwrites() {
                if let PermissionOverwriteType::Role(role) = overwrite.kind {
                    if role.0 != self.id.0 && !member.roles.contains(&role) {
                        continue;
                    }

                    if let Some(role) = cache.role(role) {
                        data.push((role.position, overwrite.deny, overwrite.allow));
                    }
                }
            }

            data.sort_by(|a, b| a.0.cmp(&b.0));

            for overwrite in data {
                permissions = (permissions & !overwrite.1) | overwrite.2;
            }

            // Member
            for overwrite in channel.permission_overwrites() {
                if PermissionOverwriteType::Member(user_id) != overwrite.kind {
                    continue;
                }

                permissions = (permissions & !overwrite.deny) | overwrite.allow;
            }
        } else {
            tracing::warn!("Guild {} does not contain channel {}", self.id, channel_id);
        }

        // The default channel is always readable.
        if channel_id.0 == self.id.0 {
            permissions |= Permissions::VIEW_CHANNEL;
        }

        remove_unusable_permissions(&mut permissions);

        permissions
    }
}

fn remove_unusable_permissions(permissions: &mut Permissions) {
    // No SEND_MESSAGES => no message-sending-related actions
    // If the member does not have the `SEND_MESSAGES` permission, then
    // throw out message-able permissions.
    if !permissions.contains(Permissions::SEND_MESSAGES) {
        *permissions &= !(Permissions::SEND_TTS_MESSAGES
            | Permissions::MENTION_EVERYONE
            | Permissions::EMBED_LINKS
            | Permissions::ATTACH_FILES);
    }

    // If the permission does not have the `READ_MESSAGES` permission, then
    // throw out actionable permissions.
    if !permissions.contains(Permissions::VIEW_CHANNEL) {
        *permissions &= Permissions::KICK_MEMBERS
            | Permissions::BAN_MEMBERS
            | Permissions::ADMINISTRATOR
            | Permissions::MANAGE_GUILD
            | Permissions::CHANGE_NICKNAME
            | Permissions::MANAGE_NICKNAMES;
    }
}
