mod renderer;
mod styled_string;

pub use renderer::*;
pub use styled_string::{Style, StyledString};
#[cfg_attr(test, allow(unused_imports))]
use weechat::Weechat;

pub struct Weechat2;

impl Weechat2 {
    #[must_use]
    pub fn color(color_name: &str) -> &str {
        #[cfg(test)]
        return color_name;
        #[cfg(not(test))]
        return Weechat::color(color_name);
    }

    #[must_use]
    pub fn info_get(name: &str, arguments: &str) -> Option<String> {
        #[cfg(test)]
        return Some(format!("{}-{}", name, arguments));
        #[cfg(not(test))]
        return Weechat::info_get(name, arguments);
    }
}
