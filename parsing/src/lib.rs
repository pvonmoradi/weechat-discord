use once_cell::sync::Lazy;
pub use simple_ast::MarkdownNode;
use simple_ast::{regex::Regex, Parser, Rule, Styled};

pub fn parse_markdown(str: &str) -> Styled<MarkdownNode> {
    use simple_ast::markdown_rules::*;
    let rules: &[&dyn Rule<MarkdownNode>] = &[
        &Escape,
        &Newline,
        &Bold,
        &Underline,
        &Italic,
        &Strikethrough,
        &Spoiler,
        &BlockQuote::new(),
        &Code,
        &InlineCode,
        &Text,
    ];

    Parser::with_rules(rules).parse(str)
}

static LINE_SUB_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^(\d)?s/(.*?(?<!\\))/(.*?(?<!\\))(?:/|$)(\w+)?").unwrap());

#[derive(Debug)]
pub enum LineEdit<'a> {
    Sub {
        line: usize,
        old: &'a str,
        new: &'a str,
        options: Option<&'a str>,
    },
    Delete {
        line: usize,
    },
}

pub fn parse_line_edit(input: &str) -> Option<LineEdit> {
    let caps = LINE_SUB_REGEX.captures(input)?;

    let line = caps.at(1).and_then(|l| l.parse().ok()).unwrap_or(1);
    let old = caps.at(2)?;
    let new = caps.at(3)?;

    if old.is_empty() && new.is_empty() {
        Some(LineEdit::Delete { line })
    } else {
        Some(LineEdit::Sub {
            line,
            old,
            new,
            options: caps.at(4),
        })
    }
}
