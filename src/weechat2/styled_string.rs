use super::Weechat2;
use itertools::{Itertools, Position};
use std::{
    collections::{Bound, VecDeque},
    ops::RangeBounds,
};

#[derive(Clone, Debug, PartialEq)]
pub enum Style {
    Bold,
    Underline,
    Italic,
    Reset,
    Color(String),
}

impl Style {
    pub fn color(color: &str) -> Self {
        Self::Color(color.to_owned())
    }

    fn style(&self) -> &str {
        match self {
            Style::Bold => Weechat2::color("bold"),
            Style::Underline => Weechat2::color("underline"),
            Style::Italic => Weechat2::color("italic"),
            Style::Reset => Weechat2::color("reset"),
            Style::Color(color) => Weechat2::color(color),
        }
    }

    fn exact_unstyle(&self) -> Option<&str> {
        match self {
            Style::Bold => Some(Weechat2::color("-bold")),
            Style::Underline => Some(Weechat2::color("-underline")),
            Style::Italic => Some(Weechat2::color("-italic")),
            Style::Reset | Style::Color(_) => None,
        }
    }
}

#[derive(Clone, Debug)]
struct StyleState {
    bold: bool,
    italic: bool,
    underline: bool,
    color: Option<String>,
}

impl StyleState {
    pub fn new() -> Self {
        Self {
            bold: false,
            italic: false,
            underline: false,
            color: None,
        }
    }

    pub fn apply(&mut self, style: &Style) {
        match style {
            Style::Bold => self.bold = true,
            Style::Underline => self.underline = true,
            Style::Italic => self.italic = true,
            Style::Reset => {
                self.bold = false;
                self.italic = false;
                self.underline = false;
                self.color = None;
            },
            Style::Color(color) => self.color = Some(color.clone()),
        }
    }

    pub fn remove(&mut self, style: &Style) {
        match style {
            Style::Bold => self.bold = false,
            Style::Underline => self.underline = false,
            Style::Italic => self.italic = false,
            Style::Reset => {},
            Style::Color(_) => self.color = None,
        }
    }

    fn styles(&self) -> Vec<Style> {
        let mut out = Vec::new();
        if self.bold {
            out.push(Style::Bold);
        }

        if self.italic {
            out.push(Style::Italic);
        }

        if self.underline {
            out.push(Style::Underline);
        }

        if let Some(color) = &self.color {
            out.push(Style::Color(color.clone()));
        }

        out
    }

    pub fn style(&self) -> String {
        self.styles().iter().map(Style::style).join("")
    }

    pub fn unstyle(&self) -> String {
        let mut out = String::new();
        for style in self.styles() {
            if let Some(unstyle) = style.exact_unstyle() {
                out.push_str(unstyle);
            } else {
                return Weechat2::color("reset").to_owned();
            }
        }
        out
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Operation {
    PushStyle(Style),
    PopStyle(Style),
    Literal(String),
    Newline,
}

#[derive(Clone, Debug)]
pub struct StyledString {
    ops: VecDeque<Operation>,
    state: StyleState,
}

impl From<String> for StyledString {
    fn from(content: String) -> Self {
        let mut out = Self::new();
        out.push_str(&content);
        out
    }
}

impl From<&str> for StyledString {
    fn from(content: &str) -> Self {
        let mut out = Self::new();
        out.push_str(&content);
        out
    }
}

impl Default for StyledString {
    fn default() -> Self {
        Self::new()
    }
}

impl StyledString {
    pub fn new() -> Self {
        Self {
            ops: VecDeque::new(),
            state: StyleState::new(),
        }
    }

    fn push_op(&mut self, operation: Operation) {
        self.ops.push_back(operation);
    }

    pub fn push_style(&mut self, style: Style) -> &mut Self {
        self.state.apply(&style);
        self.push_op(Operation::PushStyle(style));
        self
    }

    pub fn pop_style(&mut self, style: Style) -> &mut Self {
        if self.state.styles().contains(&style) {
            self.push_op(Operation::PopStyle(style));
        }
        self
    }

    pub fn push_str(&mut self, str: &str) -> &mut Self {
        for line in str.split('\n').with_position() {
            match line {
                Position::Middle(_) | Position::Last(_) => {
                    self.push_op(Operation::Newline);
                },
                _ => {},
            }
            self.push_op(Operation::Literal(line.into_inner().to_owned()));
        }
        self
    }

    pub fn push_styled_str(&mut self, style: Style, str: &str) -> &mut Self {
        self.state.apply(&style);
        self.push_op(Operation::PushStyle(style.clone()));
        self.push_str(str);
        self.push_op(Operation::PopStyle(style));
        self
    }

    /// Add some other possibly styled text without it affecting the current styling
    pub fn absorb(&mut self, other: Self) -> &mut Self {
        // add other string
        self.ops.extend(other.ops);
        // clear other string style
        self.ops.extend(
            other
                .state
                .styles()
                .into_iter()
                .rev()
                .map(Operation::PopStyle),
        );
        self
    }

    /// Adds a styled string to the end of this string
    pub fn append(&mut self, other: Self) -> &mut Self {
        self.ops.extend(other.ops);
        self.state = other.state;
        self
    }

    pub fn build(&self) -> String {
        let mut out = String::new();

        let mut state = StyleState::new();
        let mut style_stack = Vec::new();

        for token in &self.ops {
            match &token {
                Operation::PushStyle(style) => {
                    if !style_stack.contains(style) {
                        state.apply(style);
                        style_stack.push(style.clone());
                        out.push_str(style.style());
                    }
                },
                Operation::PopStyle(tstyle) => {
                    if style_stack.last() != Some(tstyle) {
                        continue;
                    }
                    if let Some(style) = style_stack.pop() {
                        state.remove(&style);
                        // TODO: Improve by using delta
                        match style.exact_unstyle() {
                            Some(unstyle) => out.push_str(unstyle),
                            None => {
                                // TODO: Optimize using "resetcolor"?
                                out.push_str(Weechat2::color("reset"));
                                out.push_str(&collect_styles(&style_stack).style());
                            },
                        }
                    }
                },
                Operation::Literal(text) => out.push_str(&text),
                Operation::Newline => {
                    out.push('\n');
                    out.push_str(&collect_styles(&style_stack).style());
                },
            }
        }

        out + &state.unstyle()
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    // TODO: Make this an iterator
    pub fn lines(self, closed: bool) -> Vec<StyledString> {
        let mut out = Vec::new();

        let mut state = StyleState::new();

        let mut tmp = StyledString::new();
        for token in self.ops {
            match &token {
                Operation::PushStyle(style) => {
                    state.apply(style);
                    tmp.push_op(token);
                },
                Operation::PopStyle(style) => {
                    state.remove(style);
                    tmp.push_op(token);
                },
                Operation::Literal(text) => {
                    debug_assert!(!text.contains('\n'));
                    tmp.push_op(token);
                },
                Operation::Newline => {
                    if closed {
                        tmp.ops
                            .extend(state.styles().into_iter().rev().map(Operation::PopStyle));
                        tmp.state = StyleState::new();
                    }
                    out.push(tmp.clone());
                    tmp = StyledString::new();
                    tmp.ops
                        .extend(state.styles().into_iter().map(Operation::PushStyle));
                    tmp.state = state.clone();
                },
            }
        }

        if !tmp.is_empty() {
            out.push(tmp);
        }

        out
    }

    pub fn find(&self, substr: &str) -> Option<usize> {
        let mut offset = 0;
        for op in &self.ops {
            if let Operation::Literal(text) = op {
                if let Some(pos) = text.find(substr) {
                    return Some(pos + offset);
                }

                offset += text.len();
            }
        }

        None
    }

    pub fn slice(self, range: impl RangeBounds<usize>) -> Self {
        let start = match range.start_bound() {
            Bound::Included(&idx) => idx,
            Bound::Excluded(&idx) => idx + 1,
            Bound::Unbounded => 0,
        };
        let end = match range.end_bound() {
            Bound::Included(&idx) => idx + 1,
            Bound::Excluded(&idx) => idx,
            Bound::Unbounded => usize::MAX,
        };

        let mut out_ops = VecDeque::new();
        let mut offset = 0;
        for op in self.ops {
            match op {
                Operation::PushStyle(_) | Operation::PopStyle(_) => {
                    out_ops.push_back(op);
                },
                Operation::Literal(text) => {
                    if start > text.len() + offset {
                        offset += text.len();
                        out_ops.push_back(Operation::Literal(text));
                        continue;
                    }
                    out_ops.push_back(Operation::Literal(
                        text.get((start.saturating_sub(offset))..(end - offset).min(text.len()))
                            .unwrap_or_default()
                            .to_owned(),
                    ));
                    if end > text.len() + offset {
                        offset += text.len();
                        continue;
                    }
                    break;
                },
                Operation::Newline => {
                    out_ops.push_back(op);
                    offset += 1;
                },
            }
        }

        Self {
            ops: out_ops,
            state: StyleState::new(),
        }
    }
}

fn collect_styles(stack: &[Style]) -> StyleState {
    stack.iter().fold(StyleState::new(), |mut acc, x| {
        acc.apply(x);
        acc
    })
}

#[cfg(test)]
mod test {
    use super::{Style, StyledString};
    use crate::weechat2::styled_string::Operation;

    #[test]
    fn find() {
        let mut string = StyledString::new();
        string.push_style(Style::Bold);
        string.push_str("**");
        string.push_style(Style::Italic);
        string.push_str("_Hi___");
        string.pop_style(Style::Italic);
        string.push_str(" there__**");
        string.pop_style(Style::Bold);

        assert_eq!(string.find("Hi"), Some(3));
    }

    #[test]
    fn slice() {
        let mut string = StyledString::new();
        string.push_style(Style::Bold);
        string.push_str("**");
        string.push_style(Style::Italic);
        string.push_str("_Hi___");
        string.pop_style(Style::Italic);
        string.push_str(" there__**");
        string.pop_style(Style::Bold);

        let mut target = StyledString::new();
        target.push_style(Style::Bold);
        target.push_str("**");
        target.push_style(Style::Italic);
        target.push_str("_");
        assert_eq!(string.slice(0..3).build(), target.build());
    }

    #[test]
    fn newline() {
        let mut string = StyledString::new();
        string.push_str("Foo\nBar");

        assert_eq!(
            Vec::from(string.ops),
            vec![
                Operation::Literal("Foo".into()),
                Operation::Newline,
                Operation::Literal("Bar".into()),
            ]
        );

        let mut string = StyledString::new();
        string.push_str("Foo\nBar\nBaz");

        assert_eq!(
            Vec::from(string.ops),
            vec![
                Operation::Literal("Foo".into()),
                Operation::Newline,
                Operation::Literal("Bar".into()),
                Operation::Newline,
                Operation::Literal("Baz".into()),
            ]
        );
    }

    #[test]
    fn lines() {
        let mut string = StyledString::new();
        string
            .push_str("Foo\nBar")
            .push_style(Style::Bold)
            .push_str("Baz\nSpam\nEggs");

        let target: Vec<String> = vec!["Foo", "BarboldBaz-bold", "boldSpam-bold", "boldEggs-bold"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(
            string
                .lines(true)
                .iter()
                .map(StyledString::build)
                .collect::<Vec<_>>(),
            target
        );
    }

    #[test]
    fn multiline() {
        // weechat does not continue styles after newline chars, so we must apply the style state to the beginning of lines
        let mut string = StyledString::new();
        string
            .push_str("Foo\nBar")
            .push_style(Style::Bold)
            .push_str("Baz\nSpam\nEggs");

        assert_eq!(string.build(), "Foo\nBarboldBaz\nboldSpam\nboldEggs-bold");
    }

    #[test]
    fn string_absorption() {
        let mut string = StyledString::new();
        string.push_style(Style::Italic).push_str("[prefix]");

        let mut inner_string = StyledString::new();
        inner_string
            .push_style(Style::Bold)
            .push_style(Style::Color("red".to_owned()))
            .push_str("[inner]");

        string.absorb(inner_string);

        string
            .push_str("[middle]")
            .push_style(Style::Bold)
            .push_str("[suffix]");

        assert_eq!(
            string.build(),
            "italic[prefix]boldred[inner]resetbolditalic-bold[middle]bold[suffix]-bold-italic"
        );
    }

    #[test]
    fn string_absoption_nested() {
        let mut string = StyledString::new();
        string
            .push_style(Style::Bold)
            .push_style(Style::Italic)
            .push_str("[prefix]");

        let mut inner_string = StyledString::new();
        inner_string.push_style(Style::Bold).push_str("[inner]");

        string.absorb(inner_string);

        string
            .push_str("[middle]")
            .push_style(Style::Bold)
            .push_str("[suffix]");

        assert_eq!(
            string.build(),
            "bolditalic[prefix][inner][middle][suffix]-bold-italic"
        );
    }

    #[test]
    fn string_append() {
        let mut string = StyledString::new();
        string.push_style(Style::Italic);
        string.push_str("[first]");

        let mut second_string = StyledString::new();
        second_string.push_style(Style::Bold);
        second_string.push_str("[second]");

        string.append(second_string);

        assert_eq!(string.build(), "italic[first]bold[second]-bold-italic");
    }

    #[test]
    fn built_string_cleanup() {
        // Basic case
        let mut string = StyledString::new();
        string.push_style(Style::Italic);
        string.push_str("text");

        assert_eq!(string.build(), "italictext-italic");

        // Colors are not "trivially unstyle-able"
        let mut string = StyledString::new();
        string.push_style(Style::Color("red".to_owned()));
        string.push_str("text");

        assert_eq!(string.build(), "redtextreset");

        // "non-trivially unstyle-able" states should short circuit
        let mut string = StyledString::new();
        string.push_style(Style::Italic);
        string.push_style(Style::Color("red".to_owned()));
        string.push_str("text");

        assert_eq!(string.build(), "italicredtextreset");
    }

    #[test]
    fn repeats() {
        let mut string = StyledString::new();
        string.push_str("[prefix]");
        string.push_style(Style::Bold);
        string.push_str("[one_bold]");
        string.push_style(Style::Bold);
        string.push_str("[two_bold]");
        string.pop_style(Style::Bold);
        string.push_str("[one_bold]");
        string.pop_style(Style::Bold);
        string.push_str("[suffix]");

        assert_eq!(
            &string.build(),
            "[prefix]bold[one_bold][two_bold]-bold[one_bold][suffix]"
        )
    }

    mod misc {
        use crate::weechat2::{Style, StyledString};

        #[test]
        fn nested() {
            let mut string = StyledString::new();
            string.push_style(Style::Bold);
            string.push_str("**");
            string.push_style(Style::Italic);
            string.push_str("_Hi___");
            string.pop_style(Style::Italic);
            string.push_str(" there__**");
            string.pop_style(Style::Bold);

            assert_eq!(&string.build(), "bold**italic_Hi___-italic there__**-bold");
        }

        #[test]
        fn resets_inner() {
            let mut string = StyledString::new();
            string.push_style(Style::Bold);
            string.push_str("**");
            let mut inner = StyledString::new();
            inner.push_style(Style::Italic);
            inner.push_str("_Hi___");
            inner.pop_style(Style::Italic);
            string.append(inner);
            string.push_str(" there__**");
            string.pop_style(Style::Bold);

            assert_eq!(&string.build(), "bold**italic_Hi___-italic there__**-bold");
        }

        #[test]
        fn extend() {
            let mut string = StyledString::new();
            string.push_str("[prefix]");
            string.push_style(Style::Bold);
            string.push_str("[outer]");
            let mut other = StyledString::new();
            other.push_style(Style::Italic);
            other.push_str("[other]");
            string.append(other.clone());

            assert_eq!(
                &string.build(),
                "[prefix]bold[outer]italic[other]-bold-italic"
            );
        }

        #[test]
        fn newline() {
            let mut string = StyledString::new();
            string.push_str("[prefix]");
            string.push_style(Style::Bold);
            string.push_str("[bold]\n[still bold]");
            string.pop_style(Style::Bold);

            assert_eq!(&string.build(), "[prefix]bold[bold]\nbold[still bold]-bold");
        }

        #[test]
        fn single_newline() {
            let mut string = StyledString::new();
            string.push_str("[prefix]");
            string.push_style(Style::Bold);
            string.push_str("[bold]");
            string.push_str("\n");
            string.push_str("[still bold]");
            string.pop_style(Style::Bold);

            assert_eq!(&string.build(), "[prefix]bold[bold]\nbold[still bold]-bold");
        }
    }
}
