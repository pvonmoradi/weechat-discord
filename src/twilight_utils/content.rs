use crate::{
    twilight_utils::{ext::ChannelExt, Color, Mentionable},
    utils::color::colorize_string,
};
use once_cell::sync::Lazy;
use regex::Regex;
use twilight::{
    cache_inmemory::InMemoryCache as Cache,
    model::id::{ChannelId, EmojiId, GuildId, RoleId, UserId},
};

pub fn clean_all(
    cache: &Cache,
    guild_id: Option<GuildId>,
    input: &str,
    unknown_members: &mut Vec<UserId>,
) -> String {
    let mut out = clean_roles(cache, input);
    out = clean_channels(cache, &out);
    out = clean_users(cache, guild_id, &out, unknown_members);
    out = clean_emojis(cache, &out);
    out
}

pub fn clean_roles(cache: &Cache, input: &str) -> String {
    let mut out = String::from(input);

    static ROLE_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"<@&(\d+?)>").expect("valid regex"));

    for role_match in ROLE_REGEX.captures_iter(input) {
        let id = role_match
            .get(1)
            .expect("Regex contains one required group");

        let id = RoleId(
            id.as_str()
                .parse::<u64>()
                .expect("Match contains only digits"),
        );

        if let Some(role) = cache.role(id) {
            out = out.replace(
                role_match.get(0).expect("match must exist").as_str(),
                &colorize_string(
                    &format!("@{}", role.name),
                    &Color::new(role.color).as_8bit().to_string(),
                ),
            );
        } else {
            out = out.replace(
                role_match.get(0).expect("match must exist").as_str(),
                "@unknown-role",
            )
        }
    }

    out
}

pub fn clean_channels(cache: &Cache, input: &str) -> String {
    let mut out = String::from(input);

    static CHANNEL_REGEX: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"<#(\d+?)>").expect("valid regex"));

    for channel_match in CHANNEL_REGEX.captures_iter(input) {
        let id = channel_match
            .get(1)
            .expect("Regex contains one required group");

        let id = ChannelId(
            id.as_str()
                .parse::<u64>()
                .expect("Match contains only digits"),
        );

        // TODO: Other channel types
        if let Some(channel) = cache.guild_channel(id) {
            out = out.replace(
                channel_match.get(0).expect("match must exist").as_str(),
                &format!("#{}", channel.name()),
            );
            continue;
        }

        if let Some(channel) = cache.private_channel(id) {
            out = out.replace(
                channel_match.get(0).expect("match must exist").as_str(),
                &format!("#{}", channel.name()),
            );
            continue;
        }

        if let Some(channel) = cache.group(id) {
            out = out.replace(
                channel_match.get(0).expect("match must exist").as_str(),
                &format!("#{}", channel.name()),
            );
            continue;
        }

        out = out.replace(
            channel_match.get(0).expect("match must exist").as_str(),
            "#unknown-channel",
        )
    }

    out
}

pub fn clean_users(
    cache: &Cache,
    guild_id: Option<GuildId>,
    input: &str,
    unknown_members: &mut Vec<UserId>,
) -> String {
    let mut out = String::from(input);

    static USER_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"<@!?(\d+?)>").expect("valid regex"));

    for user_match in USER_REGEX.captures_iter(input) {
        let id = user_match
            .get(1)
            .expect("Regex contains one required group");

        let id = UserId(
            id.as_str()
                .parse::<u64>()
                .expect("Match contains only digits"),
        );

        let replacement = if let Some(guild_id) = guild_id {
            if let Some(member) = cache.member(guild_id, id) {
                Some(crate::utils::color::colorize_discord_member(
                    cache, &member, true,
                ))
            } else {
                None
            }
        } else {
            cache.user(id).map(|user| format!("@{}", user.name))
        };
        if let Some(replacement) = replacement {
            out = out.replace(
                user_match.get(0).expect("match must exist").as_str(),
                &replacement,
            );
        } else {
            unknown_members.push(id);
            out = out.replace(
                user_match.get(0).expect("match must exist").as_str(),
                "@unknown-user",
            );
        }
    }

    out
}

pub fn clean_emojis(cache: &Cache, input: &str) -> String {
    let mut out = String::from(input);

    static EMOJI_REGEX: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"<:.+?:(\d+?)>").expect("valid regex"));

    for emoji_match in EMOJI_REGEX.captures_iter(input) {
        let id = emoji_match
            .get(1)
            .expect("Regex contains two required groups");

        let id = EmojiId(
            id.as_str()
                .parse::<u64>()
                .expect("Match contains only digits"),
        );

        if let Some(emoji) = cache.emoji(id) {
            out = out.replace(
                emoji_match.get(0).expect("match must exist").as_str(),
                &format!(":{}:", emoji.name),
            );
        } else {
            tracing::trace!(emoji.id=?id, "Emoji not in cache");
            out = out.replace(
                emoji_match.get(0).expect("match must exist").as_str(),
                ":unknown-emoji:",
            );
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
    static EMOJI_MENTIONS: Lazy<Regex> = Lazy::new(|| Regex::new(r"(.?):(\w+):").unwrap());

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
