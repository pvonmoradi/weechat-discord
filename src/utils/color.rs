use crate::twilight_utils::ext::MemberExt;
use twilight::cache::{twilight_cache_inmemory::model::CachedMember, InMemoryCache as Cache};
use weechat::Weechat;

pub fn colorize_string(text: &str, color: &str) -> String {
    if text.is_empty() || color.is_empty() {
        text.to_string()
    } else {
        format!(
            "{}{}{}",
            Weechat::color(color),
            text,
            Weechat::color("reset")
        )
    }
}

pub async fn colorize_discord_member(cache: &Cache, member: &CachedMember, at: bool) -> String {
    let color = member.color(cache).await;
    let nick = member
        .nick
        .clone()
        .unwrap_or_else(|| member.user.name.clone());

    let nick = format!("{}{}", if at { "@" } else { "" }, nick);

    color
        .map(|color| colorize_string(&nick, &color.as_8bit().to_string()))
        .unwrap_or_else(|| member.user.name.clone())
}

#[allow(dead_code)]
pub fn colorize_weechat_nick(nick: &str) -> String {
    let color = Weechat::info_get("nick_color_name", nick).unwrap_or_else(|| "reset".into());

    colorize_string(nick, &color)
}
