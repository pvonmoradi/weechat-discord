pub mod color;
mod flag;
mod format;

pub use flag::Flag;
pub use format::discord_to_weechat;

pub fn clean_name(name: &str) -> String {
    clean_name_with_case(&name.to_lowercase())
}

pub fn clean_name_with_case(name: &str) -> String {
    name.replace(' ', "_")
        .replace('\'', "")
        .replace('"', "")
        .replace('.', "")
}
