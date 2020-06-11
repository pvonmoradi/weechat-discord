use async_trait::async_trait;
use tracing::*;
use twilight::{
    cache::{twilight_cache_inmemory::model::CachedGuild, InMemoryCache as Cache},
    model::{
        channel::{
            permission_overwrite::{PermissionOverwrite, PermissionOverwriteType},
            ChannelType, GuildChannel,
        },
        guild::Permissions,
        id::{ChannelId, GuildId, RoleId, UserId},
    },
};

pub trait GuildChannelExt {
    fn name(&self) -> &str;
    fn id(&self) -> ChannelId;
    fn guild_id(&self) -> Option<GuildId>;
    fn kind(&self) -> ChannelType;
    fn permission_overwrites(&self) -> &[PermissionOverwrite];
    fn topic(&self) -> Option<String>;
}

impl GuildChannelExt for GuildChannel {
    fn name(&self) -> &str {
        match self {
            GuildChannel::Category(c) => &c.name,
            GuildChannel::Text(c) => &c.name,
            GuildChannel::Voice(c) => &c.name,
        }
    }

    fn id(&self) -> ChannelId {
        match self {
            GuildChannel::Category(c) => c.id,
            GuildChannel::Text(c) => c.id,
            GuildChannel::Voice(c) => c.id,
        }
    }
    fn guild_id(&self) -> Option<GuildId> {
        match self {
            GuildChannel::Category(c) => c.guild_id,
            GuildChannel::Text(c) => c.guild_id,
            GuildChannel::Voice(c) => c.guild_id,
        }
    }

    fn kind(&self) -> ChannelType {
        match self {
            GuildChannel::Category(c) => c.kind,
            GuildChannel::Text(c) => c.kind,
            GuildChannel::Voice(c) => c.kind,
        }
    }

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
}

#[async_trait]
pub trait CachedGuildExt {
    async fn permissions_in(
        &self,
        cache: &Cache,
        channel_id: ChannelId,
        user_id: UserId,
    ) -> Permissions;
}

#[async_trait]
impl CachedGuildExt for CachedGuild {
    async fn permissions_in(
        &self,
        cache: &Cache,
        channel_id: ChannelId,
        user_id: UserId,
    ) -> Permissions {
        // The owner has all permissions in all cases.
        if user_id == self.owner_id {
            return Permissions::all();
        }

        // Start by retrieving the @everyone role's permissions.
        let everyone = match cache
            .role(RoleId(self.id.0))
            .await
            .expect("InMemoryCache cannot fail")
        {
            Some(everyone) => everyone,
            None => {
                error!("@everyone role ({}) missing in '{}'", self.id, self.name);

                return Permissions::empty();
            },
        };

        // Create a base set of permissions, starting with `@everyone`s.
        let mut permissions = everyone.permissions;

        let member = match cache
            .member(self.id, user_id)
            .await
            .expect("InMemoryCache cannot fail")
        {
            Some(member) => member,
            None => return everyone.permissions,
        };

        for &role in &member.roles {
            if let Some(role) = cache.role(role).await.expect("InMemoryCache cannot fail") {
                permissions |= role.permissions;
            } else {
                warn!(
                    "{} on {} has non-existent role {:?}",
                    member.user.id, self.id, role
                );
            }
        }

        // Administrators have all permissions in any channel.
        if permissions.contains(Permissions::ADMINISTRATOR) {
            return Permissions::all();
        }

        if let Some(channel) = cache
            .guild_channel(channel_id)
            .await
            .expect("InMemoryCache cannot fail")
        {
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

                    if let Some(role) = cache.role(role).await.expect("InMemoryCache cannot fail") {
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
            warn!("Guild {} does not contain channel {}", self.id, channel_id);
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
