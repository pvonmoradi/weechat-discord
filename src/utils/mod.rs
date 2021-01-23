pub mod color;
mod flag;
mod format;
#[cfg(feature = "images")]
pub mod image;

pub use flag::Flag;
pub use format::{discord_to_weechat, fold_lines};

#[macro_export]
macro_rules! match_map {
    ($expression:expr, $( $pattern:pat )|+ $( if $guard: expr )? => $v:expr $(,)?) => {
        match $expression {
            $( $pattern )|+ $( if $guard )? => Some($v),
            _ => None
        }
    }
}

pub fn clean_name(name: &str) -> String {
    clean_name_with_case(&name.to_lowercase())
}

pub fn clean_name_with_case(name: &str) -> String {
    name.replace(' ', "_")
        .replace('\'', "")
        .replace('"', "")
        .replace('.', "")
}
