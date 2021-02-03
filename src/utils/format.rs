use crate::{
    twilight_utils::{ext::ChannelExt, Color},
    utils::color::colorize_string,
    Weechat2,
};
use parsing::MarkdownNode;
use std::{rc::Rc, sync::RwLock};
use twilight_cache_inmemory::InMemoryCache as Cache;
use twilight_model::id::{GuildId, UserId};

struct FormattingState<'a> {
    cache: &'a Cache,
    guild_id: Option<GuildId>,
    show_unknown_ids: bool,
    unknown_members: &'a mut Vec<UserId>,
    color_stack: &'a mut Vec<&'static str>,
}

pub fn discord_to_weechat(
    msg: &str,
    cache: &Cache,
    guild_id: Option<GuildId>,
    show_unknown_ids: bool,
    unknown_members: &mut Vec<UserId>,
) -> String {
    let mut state = FormattingState {
        cache,
        guild_id,
        show_unknown_ids,
        unknown_members,
        color_stack: &mut Vec::new(),
    };
    let ast = parsing::parse_markdown(msg);

    collect_styles(&ast.0, &mut state)
}

fn collect_styles(styles: &[Rc<RwLock<MarkdownNode>>], state: &mut FormattingState) -> String {
    styles
        .iter()
        .map(|s| discord_to_weechat_reducer(&*s.read().unwrap(), state))
        .collect::<Vec<_>>()
        .join("")
}

fn push_color(color: &'static str, state: &mut FormattingState) -> &'static str {
    state.color_stack.push(color);
    Weechat2::color(color)
}

fn pop_color(state: &mut FormattingState) -> String {
    state.color_stack.pop();
    let mut out = Weechat2::color("resetcolor").to_string();
    for color in state.color_stack.iter() {
        out.push_str(Weechat2::color(color));
    }

    out
}

// TODO: if the whole line is wrapped in *, render as CTCP ACTION rather than
//       as fully italicized message.
fn discord_to_weechat_reducer(node: &MarkdownNode, state: &mut FormattingState) -> String {
    use MarkdownNode::*;
    match node {
        Bold(styles) => format!(
            "{}**{}**{}",
            Weechat2::color("bold"),
            collect_styles(styles, state),
            Weechat2::color("-bold")
        ),
        Italic(styles) => format!(
            "{}_{}_{}",
            Weechat2::color("italic"),
            collect_styles(styles, state),
            Weechat2::color("-italic")
        ),
        Underline(styles) => format!(
            "{}__{}__{}",
            Weechat2::color("underline"),
            collect_styles(styles, state),
            Weechat2::color("-underline")
        ),
        Strikethrough(styles) => format!(
            "{}~~{}~~{}",
            push_color("|red", state),
            collect_styles(styles, state),
            pop_color(state)
        ),
        Spoiler(styles) => format!(
            "{}||{}||{}",
            Weechat2::color("italic"),
            collect_styles(styles, state),
            Weechat2::color("-italic")
        ),
        Text(string) => string.to_owned(),
        InlineCode(string) => format!(
            "{}`{}`{}{}",
            push_color("|*8", state),
            string,
            Weechat2::color("-bold"),
            pop_color(state)
        ),
        Code(language, text) => {
            let (fmt, reset) = (
                push_color("|*8", state),
                pop_color(state) + Weechat2::color("-bold"),
            );

            #[cfg(feature = "syntax_highlighting")]
            let text = syntax::format_code(text, language);

            format!(
                "```{}\n{}\n```",
                language,
                text.lines()
                    .map(|l| format!("{}{}{}", fmt, l, reset))
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
        },
        BlockQuote(styles) => format_block_quote(collect_styles(styles, state).lines()),
        SingleBlockQuote(styles) => format_block_quote(
            collect_styles(styles, state)
                .lines()
                .map(strip_leading_bracket),
        ),
        UserMention(id) => {
            let id = (*id).into();
            let replacement = if let Some(guild_id) = state.guild_id {
                if let Some(member) = state.cache.member(guild_id, id) {
                    Some(crate::utils::color::colorize_discord_member(
                        state.cache,
                        &member,
                        true,
                    ))
                } else {
                    None
                }
            } else {
                state.cache.user(id).map(|user| {
                    format!(
                        "@{}",
                        crate::utils::color::colorize_weechat_nick(&user.name)
                    )
                })
            };

            if let Some(replacement) = replacement {
                replacement
            } else {
                state.unknown_members.push(id);

                if state.show_unknown_ids {
                    format!("@{}", id.0)
                } else {
                    "@unknown-user".into()
                }
            }
        },
        ChannelMention(id) => {
            let id = (*id).into();
            if let Some(channel) = state.cache.guild_channel(id) {
                return format!("#{}", channel.name());
            }

            if let Some(channel) = state.cache.private_channel(id) {
                return format!("#{}", channel.name());
            }

            if let Some(channel) = state.cache.group(id) {
                return format!("#{}", channel.name());
            }

            "#unknown-channel".to_owned()
        },
        Emoji(_, id) => {
            if let Some(emoji) = state.cache.emoji((*id).into()) {
                format!(":{}:", emoji.name)
            } else {
                tracing::trace!(emoji.id=?id, "Emoji not in cache");
                ":unknown-emoji:".to_owned()
            }
        },
        RoleMention(id) => {
            if let Some(role) = state.cache.role((*id).into()) {
                colorize_string(
                    &format!("@{}", role.name),
                    &Color::new(role.color).as_8bit().to_string(),
                )
            } else {
                format!("@unknown-role")
            }
        },
    }
}

#[cfg(feature = "syntax_highlighting")]
mod syntax {
    use crate::{twilight_utils::Color, Weechat2};
    use once_cell::sync::Lazy;
    use syntect::{
        easy::HighlightLines,
        highlighting::{Style, ThemeSet},
        parsing::SyntaxSet,
        util::LinesWithEndings,
    };

    pub fn format_code(src: &str, language: &str) -> String {
        static PS: Lazy<SyntaxSet> = Lazy::new(SyntaxSet::load_defaults_newlines);
        static TS: Lazy<ThemeSet> = Lazy::new(ThemeSet::load_defaults);

        if let Some(syntax) = PS.find_syntax_by_token(language) {
            let mut h = HighlightLines::new(syntax, &TS.themes["Solarized (dark)"]);
            let mut out = String::new();
            for line in LinesWithEndings::from(src) {
                let ranges: Vec<(Style, &str)> = h.highlight(line, &PS);
                out.push_str(&syntect_as_weechat_escaped(&ranges[..]));
            }
            out
        } else {
            tracing::debug!("unable to find syntax for language: {}", language);
            src.to_string()
        }
    }

    fn syntect_as_weechat_escaped(v: &[(Style, &str)]) -> String {
        let mut o = String::new();
        let resetcolor = Weechat2::color("resetcolor");
        for (style, str) in v {
            let fg = style.foreground;
            let fg = Color::from_rgb(fg.r, fg.g, fg.b);
            let colorstr = format!("{}", fg.as_8bit());
            let color = Weechat2::color(&colorstr);
            o.push_str(&format!("{}{}{}", color, str, resetcolor));
        }
        o
    }
}

fn strip_leading_bracket(line: &str) -> &str {
    &line[line.find("> ").map(|x| x + 2).unwrap_or(0)..]
}

pub fn fold_lines<'a>(lines: impl Iterator<Item = &'a str>, sep: &'a str) -> String {
    lines.fold(String::new(), |acc, x| format!("{}{}{}\n", acc, sep, x))
}

fn format_block_quote<'a>(lines: impl Iterator<Item = &'a str>) -> String {
    fold_lines(lines, "▎")
}

#[cfg(test)]
mod tests {
    use super::discord_to_weechat;
    use twilight_cache_inmemory::InMemoryCache as Cache;
    use twilight_model::{
        channel::{Channel, ChannelType, GuildChannel, TextChannel},
        gateway::payload::{ChannelCreate, GuildEmojisUpdate, MemberAdd, RoleCreate},
        guild::{Emoji, Member, Permissions, Role},
        id::{ChannelId, EmojiId, GuildId, RoleId, UserId},
        user::User,
    };

    fn format(str: &str) -> String {
        format_with_cache(str, &Cache::new(), None)
    }

    fn format_with_cache(str: &str, cache: &Cache, guild_id: Option<GuildId>) -> String {
        discord_to_weechat(str, cache, guild_id, false, &mut Vec::new())
    }

    #[test]
    fn color_stack() {
        assert_eq!(
            format("||foo ~~strikethrough~~ baz `code` spam||"),
            "italic||foo |red~~strikethrough~~resetcolor baz |*8`code`-boldresetcolor spam||-italic"
        );
    }

    #[test]
    fn smoke_test() {
        assert_eq!(
            format("**_Hi___ there__**"),
            "bold**italic_Hi___-italic there__**-bold"
        );
        assert_eq!(format("A _b*c_d*e_"), "A _bitalic_c_d_-italice_");
        assert_eq!(
            format("__f_*o*_o__"),
            "underline__f_italic_o_-italic_o__-underline"
        )
    }

    #[test]
    fn roles() {
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
            format_with_cache("hello <@&1>!", &cache, None),
            "hello 16@fooresetcolor!"
        );
    }

    #[test]
    fn channels() {
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
            format_with_cache("hello <#1>!", &cache, guild_id),
            "hello #channel-one!"
        );
    }

    // TODO: Expand this, to test members, users, show_unkown, and the unknown_users aspects
    #[test]
    fn users() {
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
            format_with_cache("hello <@1>!", &cache, Some(guild_id)),
            "hello @random-user!"
        );
        assert_eq!(
            format_with_cache("hello <@!1>!", &cache, Some(guild_id)),
            "hello @random-user!"
        );
    }

    #[test]
    fn emojis() {
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
            format_with_cache("hello <:random-emoji:1> <:emoji-two:2>", &cache, None,),
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
        assert_eq!(format_with_cache(src, &cache, None), target);
    }
}
