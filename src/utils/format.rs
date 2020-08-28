use parsing::MarkdownNode;
use std::{rc::Rc, sync::RwLock};
use weechat::Weechat;

pub fn discord_to_weechat(msg: &str) -> String {
    let ast = parsing::parse_markdown(msg);

    let mut out = String::new();
    for node in &ast.0 {
        out.push_str(&discord_to_weechat_reducer(&*node.read().unwrap()))
    }
    out
}

fn collect_styles(styles: &[Rc<RwLock<MarkdownNode>>]) -> String {
    styles
        .iter()
        .map(|s| discord_to_weechat_reducer(&*s.read().unwrap()))
        .collect::<Vec<_>>()
        .join("")
}

// TODO: if the whole line is wrapped in *, render as CTCP ACTION rather than
//       as fully italicized message.
fn discord_to_weechat_reducer(node: &MarkdownNode) -> String {
    use MarkdownNode::*;
    match node {
        Bold(styles) => format!(
            "{}{}{}",
            Weechat::color("bold"),
            collect_styles(styles),
            Weechat::color("-bold")
        ),
        Italic(styles) => format!(
            "{}{}{}",
            Weechat::color("italic"),
            collect_styles(styles),
            Weechat::color("-italic")
        ),
        Underline(styles) => format!(
            "{}{}{}",
            Weechat::color("underline"),
            collect_styles(styles),
            Weechat::color("-underline")
        ),
        Strikethrough(styles) => format!(
            "{}~~{}~~{}",
            Weechat::color("red"),
            collect_styles(styles),
            Weechat::color("-red")
        ),
        Spoiler(styles) => format!(
            "{}||{}||{}",
            Weechat::color("italic"),
            collect_styles(styles),
            Weechat::color("-italic")
        ),
        Text(string) => string.to_owned(),
        InlineCode(string) => format!(
            "{}{}{}",
            Weechat::color("*8"),
            string,
            Weechat::color("reset")
        ),
        Code(language, text) => {
            let (fmt, reset) = (Weechat::color("*8"), Weechat::color("reset"));

            format!(
                "```{}\n{}\n```",
                language,
                text.lines()
                    .map(|l| format!("{}{}{}", fmt, l, reset))
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
        },
        BlockQuote(styles) => format_block_quote(collect_styles(styles).lines()),
        SingleBlockQuote(styles) => {
            format_block_quote(collect_styles(styles).lines().map(strip_leading_bracket))
        },
    }
}

fn strip_leading_bracket(line: &str) -> &str {
    &line[line.find("> ").map(|x| x + 2).unwrap_or(0)..]
}

fn format_block_quote<'a>(lines: impl Iterator<Item = &'a str>) -> String {
    lines.fold(String::new(), |acc, x| format!("{}▎{}\n", acc, x))
}
