pub mod color;
mod flag;
mod format;

pub use flag::Flag;
pub use format::discord_to_weechat;

pub fn clean_name(name: &str) -> String {
    name.to_lowercase()
        .replace(' ', "_")
        .replace('\'', "")
        .replace('"', "")
        .replace('.', "")
}
