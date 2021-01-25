use crate::{
    twilight_utils::{ext::ChannelExt, Color, Mentionable},
    utils::color::colorize_string,
};
use once_cell::sync::Lazy;
use regex::Regex;
use twilight_cache_inmemory::InMemoryCache as Cache;
use twilight_mention::{parse::MentionType, ParseMention};
use twilight_model::id::{GuildId, UserId};

pub fn clean_all(
    cache: &Cache,
    input: &str,
    guild_id: Option<GuildId>,
    show_unknown_ids: bool,
    unknown_members: &mut Vec<UserId>,
) -> String {
    let mut out = String::from(input);
    for (mention, start, end) in twilight_mention::parse::MentionType::iter(input) {
        let end = end + 1;
        // TODO: Optimize this. since we know the bounds, we should be able to slice and replace
        //       more efficiently than just using .replace
        match mention {
            MentionType::Role(id) => {
                if let Some(role) = cache.role(id) {
                    out = out.replace(
                        &input[start..end],
                        &colorize_string(
                            &format!("@{}", role.name),
                            &Color::new(role.color).as_8bit().to_string(),
                        ),
                    );
                } else {
                    out = out.replace(&input[start..end], "@unknown-role")
                }
            },
            MentionType::Channel(id) => {
                if let Some(channel) = cache.guild_channel(id) {
                    out = out.replace(&input[start..end], &format!("#{}", channel.name()));
                    continue;
                }

                if let Some(channel) = cache.private_channel(id) {
                    out = out.replace(&input[start..end], &format!("#{}", channel.name()));
                    continue;
                }

                if let Some(channel) = cache.group(id) {
                    out = out.replace(&input[start..end], &format!("#{}", channel.name()));
                    continue;
                }

                out = out.replace(&input[start..end], "#unknown-channel")
            },
            MentionType::User(id) => {
                let replacement = if let Some(guild_id) = guild_id {
                    if let Some(member) = cache.member(guild_id, id) {
                        Some(crate::utils::color::colorize_discord_member(
                            cache, &member, true,
                        ))
                    } else {
                        None
                    }
                } else {
                    cache.user(id).map(|user| {
                        format!(
                            "@{}",
                            crate::utils::color::colorize_weechat_nick(&user.name)
                        )
                    })
                };
                if let Some(replacement) = replacement {
                    out = out.replace(&input[start..end], &replacement);
                } else {
                    unknown_members.push(id);
                    out = out.replace(
                        &input[start..end],
                        &if show_unknown_ids {
                            format!("@{}", id.0)
                        } else {
                            "@unknown-user".into()
                        },
                    );
                }
            },
            MentionType::Emoji(id) => {
                if let Some(emoji) = cache.emoji(id) {
                    out = out.replace(&input[start..end], &format!(":{}:", emoji.name));
                } else {
                    tracing::trace!(emoji.id=?id, "Emoji not in cache");
                    out = out.replace(&input[start..end], ":unknown-emoji:");
                }
            },
            _ => unreachable!("exhaustive"),
        }
    }

    out
}

pub fn create_mentions(cache: &Cache, guild_id: Option<GuildId>, input: &str) -> String {
    let mut out = create_channels(cache, guild_id, input);
    out = create_users(cache, guild_id, &out);
    out = create_roles(cache, guild_id, &out);
    out = create_emojis(cache, guild_id, &out);

    out
}

pub fn create_channels(cache: &Cache, guild_id: Option<GuildId>, input: &str) -> String {
    let mut out = String::from(input);
    static CHANNEL_MENTION: Lazy<Regex> = Lazy::new(|| Regex::new(r"#([a-z_\-\d]+)").unwrap());

    let matches = CHANNEL_MENTION.captures_iter(&input).collect::<Vec<_>>();
    for channel_match in matches {
        let channel_name = channel_match
            .get(1)
            .expect("Regex contains exactly one group")
            .as_str();

        if let Some(guild_id) = guild_id {
            if let Some(channel_ids) = cache.channel_ids_in_guild(guild_id) {
                for channel_id in channel_ids {
                    if let Some(channel) = cache.guild_channel(channel_id) {
                        if channel.name() == channel_name {
                            out = out.replace(
                                channel_match
                                    .get(0)
                                    .expect("group zero must exist")
                                    .as_str(),
                                &channel.id().mention(),
                            )
                        }
                    }
                }
            }
        }
    }

    out
}

pub fn create_users(cache: &Cache, guild_id: Option<GuildId>, input: &str) -> String {
    let mut out = String::from(input);
    static USER_MENTION: Lazy<Regex> = Lazy::new(|| Regex::new(r"@(.{0,32}?)#(\d{2,4})").unwrap());

    let matches = USER_MENTION.captures_iter(input).collect::<Vec<_>>();
    for user_match in matches {
        let user_name = user_match
            .get(1)
            .expect("Regex contains exactly one group")
            .as_str();

        if let Some(guild_id) = guild_id {
            if let Some(members) = cache.members(guild_id) {
                for member in members {
                    if let Some(nick) = &member.nick {
                        if nick == user_name {
                            out = out.replace(
                                user_match.get(0).expect("group zero must exist").as_str(),
                                &member.user.id.mention(),
                            );
                        }
                    }

                    if member.user.name == user_name {
                        out = out.replace(
                            user_match.get(0).expect("group zero must exist").as_str(),
                            &member.user.id.mention(),
                        );
                    }
                }
            }
        }
    }

    out
}

pub fn create_roles(cache: &Cache, guild_id: Option<GuildId>, input: &str) -> String {
    let mut out = String::from(input);
    static ROLE_MENTION: Lazy<Regex> = Lazy::new(|| Regex::new(r"@([^\s]{1,32})").unwrap());

    let matches = ROLE_MENTION.captures_iter(input).collect::<Vec<_>>();
    for role_match in matches {
        let role_name = role_match
            .get(1)
            .expect("Regex contains exactly one group")
            .as_str();

        if let Some(guild_id) = guild_id {
            if let Some(roles) = cache.roles(guild_id) {
                for role_id in roles {
                    if let Some(role) = cache.role(role_id) {
                        if role.name == role_name {
                            out = out.replace(
                                role_match.get(0).expect("group zero must exist").as_str(),
                                &role_id.mention(),
                            )
                        }
                    }
                }
            }
        }
    }

    out
}

pub fn create_emojis(cache: &Cache, guild_id: Option<GuildId>, input: &str) -> String {
    let mut out = String::from(input);
    static EMOJI_MENTIONS: Lazy<Regex> = Lazy::new(|| Regex::new(r"(\\?):(\w+):").unwrap());

    let matches = EMOJI_MENTIONS.captures_iter(input).collect::<Vec<_>>();
    for emoji_match in matches {
        let emoji_prefix = emoji_match
            .get(1)
            .expect("Regex contains two groups")
            .as_str();

        if emoji_prefix == "\\" {
            continue;
        }

        let emoji_name = emoji_match
            .get(2)
            .expect("Regex contains two groups")
            .as_str();
        if let Some(guild_id) = guild_id {
            if let Some(emojis) = cache.emojis(guild_id) {
                for emoji_id in emojis {
                    if let Some(emoji) = cache.emoji(emoji_id) {
                        if emoji.name == emoji_name {
                            out = out.replace(
                                emoji_match.get(0).expect("group zero must exist").as_str(),
                                &emoji.mention(),
                            )
                        }
                    }
                }
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use twilight_cache_inmemory::InMemoryCache as Cache;
    use twilight_model::{
        channel::{Channel, ChannelType, GuildChannel, TextChannel},
        gateway::payload::{ChannelCreate, GuildEmojisUpdate, MemberAdd, RoleCreate},
        guild::{Emoji, Member, Permissions, Role},
        id::{ChannelId, EmojiId, RoleId},
        user::User,
    };

    #[tokio::test]
    async fn roles() {
        let cache = Cache::new();
        let role = Role {
            color: 0,
            hoist: false,
            id: RoleId(1),
            managed: false,
            mentionable: false,
            name: "foo".to_string(),
            permissions: Permissions::CREATE_INVITE,
            position: 0,
            tags: None,
        };
        cache.update(&RoleCreate {
            guild_id: GuildId(0),
            role,
        });

        assert_eq!(
            clean_all(&cache, "hello <@&1>!", None, false, &mut vec![]),
            "hello 16@fooreset!"
        );
    }

    #[tokio::test]
    async fn channels() {
        let cache = Cache::new();
        let guild_id = Some(GuildId(0));
        let channel = GuildChannel::Text(TextChannel {
            guild_id,
            id: ChannelId(1),
            kind: ChannelType::GuildText,
            last_message_id: None,
            last_pin_timestamp: None,
            name: "channel-one".to_string(),
            nsfw: false,
            permission_overwrites: vec![],
            parent_id: None,
            position: 0,
            rate_limit_per_user: None,
            topic: None,
        });
        cache.update(&ChannelCreate(Channel::Guild(channel)));

        assert_eq!(
            clean_all(&cache, "hello <#1>!", guild_id, false, &mut vec![]),
            "hello #channel-one!"
        );
    }

    // TODO: Expand this, to test members, users, show_unkown, and the unknown_users aspects
    #[tokio::test]
    async fn users() {
        let guild_id = GuildId(0);

        let cache = Cache::new();
        let member = Member {
            deaf: false,
            guild_id,
            hoisted_role: None,
            joined_at: None,
            mute: false,
            nick: None,
            pending: false,
            premium_since: None,
            roles: vec![],
            user: User {
                avatar: None,
                bot: false,
                discriminator: "1234".to_string(),
                email: None,
                flags: None,
                id: UserId(1),
                locale: None,
                mfa_enabled: None,
                name: "random-user".to_string(),
                premium_type: None,
                public_flags: None,
                system: None,
                verified: None,
            },
        };
        cache.update(&MemberAdd(member));

        assert_eq!(
            clean_all(
                &cache,
                "hello <@1>!",
                Some(guild_id),
                false,
                &mut Vec::new(),
            ),
            "hello @random-user!"
        );
        assert_eq!(
            clean_all(
                &cache,
                "hello <@!1>!",
                Some(guild_id),
                false,
                &mut Vec::new(),
            ),
            "hello @random-user!"
        );
    }

    #[tokio::test]
    async fn emojis() {
        let cache = Cache::new();
        let emojis = vec![
            Emoji {
                animated: false,
                available: false,
                id: EmojiId(1),
                managed: false,
                name: "random-emoji".to_string(),
                require_colons: false,
                roles: vec![],
                user: None,
            },
            Emoji {
                animated: false,
                available: false,
                id: EmojiId(2),
                managed: false,
                name: "emoji-two".to_string(),
                require_colons: false,
                roles: vec![],
                user: None,
            },
        ];
        cache.update(&GuildEmojisUpdate {
            emojis,
            guild_id: GuildId(0),
        });

        assert_eq!(
            clean_all(
                &cache,
                "hello <:random-emoji:1> <:emoji-two:2>",
                None,
                false,
                &mut vec![],
            ),
            "hello :random-emoji: :emoji-two:"
        );
    }

    #[test]
    fn evil_pony() {
        let cache = Cache::new();
        let mut emojis = Vec::new();
        for (i, n) in ["one", "two", "three", "four", "five", "six"]
            .iter()
            .enumerate()
        {
            let emoji = Emoji {
                animated: false,
                available: false,
                id: EmojiId(i as u64 + 1),
                managed: false,
                name: n.to_string(),
                require_colons: false,
                roles: vec![],
                user: None,
            };
            emojis.push(emoji);
        }
        cache.update(&GuildEmojisUpdate {
            emojis,
            guild_id: GuildId(0),
        });
        let src = "<:one:1><:two:2><:one:1>\
        <:three:3><:four:4><:five:5>\
        <:one:1><:six:6><:one:1>";
        let target = ":one::two::one:\
        :three::four::five:\
        :one::six::one:";
        assert_eq!(clean_all(&cache, src, None, false, &mut vec![]), target);
    }
}
