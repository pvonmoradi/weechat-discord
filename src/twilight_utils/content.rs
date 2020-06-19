use crate::{
    twilight_utils::{ext::ChannelExt, Color},
    utils::color::colorize_string,
};
use lazy_static::lazy_static;
use regex::Regex;
use twilight::{
    cache::InMemoryCache as Cache,
    model::id::{ChannelId, EmojiId, GuildId, RoleId, UserId},
};

pub async fn clean_all(
    cache: &Cache,
    guild_id: Option<GuildId>,
    input: &str,
    unknown_members: &mut Vec<UserId>,
) -> String {
    let mut out = clean_roles(cache, input).await;
    out = clean_channels(cache, &out).await;
    out = clean_users(cache, guild_id, &out, unknown_members).await;
    out = clean_emojis(cache, &out).await;
    out
}

pub async fn clean_roles(cache: &Cache, input: &str) -> String {
    let mut out = String::from(input);

    lazy_static! {
        static ref ROLE_REGEX: Regex = Regex::new(r"<@&(\d+?)>").expect("valid regex");
    }

    for role_match in ROLE_REGEX.captures_iter(input) {
        let id = role_match
            .get(1)
            .expect("Regex contains one required group");

        let id = RoleId(
            id.as_str()
                .parse::<u64>()
                .expect("Match contains only digits"),
        );

        if let Some(role) = cache.role(id).await.expect("InMemoryCache cannot fail") {
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

pub async fn clean_channels(cache: &Cache, input: &str) -> String {
    let mut out = String::from(input);

    lazy_static! {
        static ref CHANNEL_REGEX: Regex = Regex::new(r"<#(\d+?)>").expect("valid regex");
    }

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
        if let Some(channel) = cache
            .guild_channel(id)
            .await
            .expect("InMemoryCache cannot fail")
        {
            out = out.replace(
                channel_match.get(0).expect("match must exist").as_str(),
                &format!("#{}", channel.name()),
            );
            continue;
        }

        if let Some(channel) = cache
            .private_channel(id)
            .await
            .expect("InMemoryCache cannot fail")
        {
            out = out.replace(
                channel_match.get(0).expect("match must exist").as_str(),
                &format!("#{}", channel.name()),
            );
            continue;
        }

        if let Some(channel) = cache.group(id).await.expect("InMemoryCache cannot fail") {
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

pub async fn clean_users(
    cache: &Cache,
    guild_id: Option<GuildId>,
    input: &str,
    unknown_members: &mut Vec<UserId>,
) -> String {
    let mut out = String::from(input);

    lazy_static! {
        static ref USER_REGEX: Regex = Regex::new(r"<@!?(\d+?)>").expect("valid regex");
    }

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
            if let Some(member) = cache
                .member(guild_id, id)
                .await
                .expect("InMemoryCache cannot fail")
            {
                Some(crate::utils::color::colorize_discord_member(cache, &member, true).await)
            } else {
                None
            }
        } else {
            cache
                .user(id)
                .await
                .expect("InMemoryCache cannot fail")
                .map(|user| format!("@{}", user.name))
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

pub async fn clean_emojis(cache: &Cache, input: &str) -> String {
    let mut out = String::from(input);

    lazy_static! {
        static ref EMOJI_REGEX: Regex = Regex::new(r"<:.+?:(\d+?)>").expect("valid regex");
    }

    for emoji_match in EMOJI_REGEX.captures_iter(input) {
        let id = emoji_match
            .get(1)
            .expect("Regex contains two required groups");

        let id = EmojiId(
            id.as_str()
                .parse::<u64>()
                .expect("Match contains only digits"),
        );

        if let Some(emoji) = cache.emoji(id).await.expect("InMemoryCache cannot fail") {
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
