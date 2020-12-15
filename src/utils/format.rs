use crate::Weechat2;
use parsing::MarkdownNode;
use std::{rc::Rc, sync::RwLock};

pub fn discord_to_weechat(msg: &str) -> String {
    let ast = parsing::parse_markdown(msg);

    collect_styles(&ast.0, &mut Vec::new())
}

fn collect_styles(
    styles: &[Rc<RwLock<MarkdownNode>>],
    color_stack: &mut Vec<&'static str>,
) -> String {
    styles
        .iter()
        .map(|s| discord_to_weechat_reducer(&*s.read().unwrap(), color_stack))
        .collect::<Vec<_>>()
        .join("")
}

fn push_color(color: &'static str, color_stack: &mut Vec<&'static str>) -> &'static str {
    color_stack.push(color);
    Weechat2::color(color)
}

fn pop_color(color_stack: &mut Vec<&'static str>) -> String {
    color_stack.pop();
    let mut out = Weechat2::color("resetcolor").to_string();
    for color in color_stack {
        out.push_str(Weechat2::color(color));
    }

    out
}

// TODO: if the whole line is wrapped in *, render as CTCP ACTION rather than
//       as fully italicized message.
fn discord_to_weechat_reducer(node: &MarkdownNode, color_stack: &mut Vec<&'static str>) -> String {
    use MarkdownNode::*;
    match node {
        Bold(styles) => format!(
            "{}{}{}",
            Weechat2::color("bold"),
            collect_styles(styles, color_stack),
            Weechat2::color("-bold")
        ),
        Italic(styles) => format!(
            "{}{}{}",
            Weechat2::color("italic"),
            collect_styles(styles, color_stack),
            Weechat2::color("-italic")
        ),
        Underline(styles) => format!(
            "{}{}{}",
            Weechat2::color("underline"),
            collect_styles(styles, color_stack),
            Weechat2::color("-underline")
        ),
        Strikethrough(styles) => format!(
            "{}~~{}~~{}",
            push_color("|red", color_stack),
            collect_styles(styles, color_stack),
            pop_color(color_stack)
        ),
        Spoiler(styles) => format!(
            "{}||{}||{}",
            Weechat2::color("italic"),
            collect_styles(styles, color_stack),
            Weechat2::color("-italic")
        ),
        Text(string) => string.to_owned(),
        InlineCode(string) => format!(
            "{}{}{}{}",
            push_color("|*8", color_stack),
            string,
            Weechat2::color("-bold"),
            pop_color(color_stack)
        ),
        Code(language, text) => {
            let (fmt, reset) = (
                push_color("|*8", color_stack),
                pop_color(color_stack) + Weechat2::color("-bold"),
            );

            format!(
                "```{}\n{}\n```",
                language,
                text.lines()
                    .map(|l| format!("{}{}{}", fmt, l, reset))
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
        },
        BlockQuote(styles) => format_block_quote(collect_styles(styles, color_stack).lines()),
        SingleBlockQuote(styles) => format_block_quote(
            collect_styles(styles, color_stack)
                .lines()
                .map(strip_leading_bracket),
        ),
    }
}

fn strip_leading_bracket(line: &str) -> &str {
    &line[line.find("> ").map(|x| x + 2).unwrap_or(0)..]
}

fn format_block_quote<'a>(lines: impl Iterator<Item = &'a str>) -> String {
    lines.fold(String::new(), |acc, x| format!("{}▎{}\n", acc, x))
}

#[cfg(test)]
mod tests {
    use super::discord_to_weechat;

    #[test]
    fn color_stack() {
        assert_eq!(
            discord_to_weechat("||foo ~~strikethrough~~ baz `code` spam||"),
            "italic||foo |red~~strikethrough~~resetcolor baz |*8code-boldresetcolor spam||-italic"
        );
    }

    #[test]
    fn smoke_test() {
        assert_eq!(
            discord_to_weechat("**_Hi___ there__**"),
            "bolditalicHi__-italic there__-bold"
        );
        assert_eq!(discord_to_weechat("A _b*c_d*e_"), "A _bitalicc_d-italice_");
        assert_eq!(
            discord_to_weechat("__f_*o*_o__"),
            "underlinef_italico-italic_o-underline"
        )
    }
}
