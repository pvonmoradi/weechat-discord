use crate::{
    twilight_utils::ext::MemberExt,
    weechat2::{Style, StyledString},
    Weechat2,
};
use twilight_cache_inmemory::{model::CachedMember, InMemoryCache as Cache};

pub fn colorize_string(text: &str, color: &str) -> StyledString {
    let mut builder = StyledString::new();
    if text.is_empty() || color.is_empty() {
        builder.push_str(text);
    } else {
        builder.push_styled_str(Style::color(color), text);
    }
    builder
}

pub fn colorize_discord_member(cache: &Cache, member: &CachedMember, at: bool) -> StyledString {
    let color = member.color(cache);
    let nick = member
        .nick
        .clone()
        .unwrap_or_else(|| member.user.name.clone());

    let nick_prefix = if at { "@" } else { "" };
    let nick = format!("{}{}", nick_prefix, nick);

    color
        .map(|color| colorize_string(&nick, &color.as_8bit().to_string()))
        .unwrap_or_else(|| {
            StyledString::from(format!("{}{}", nick_prefix, member.user.name.clone()))
        })
}

pub fn colorize_weechat_nick(nick: &str) -> StyledString {
    let color = Weechat2::info_get("nick_color_name", nick).unwrap_or_else(|| "reset".into());

    colorize_string(nick, &color)
}
