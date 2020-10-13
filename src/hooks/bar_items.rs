use crate::{
    buffer::ext::BufferExt, config::Config, discord::typing_indicator::TypingTracker,
    instance::Instance,
};
use twilight_model::id::{ChannelId, GuildId};
use weechat::{buffer::Buffer, hooks::BarItem, Weechat};

pub struct BarItems {
    _typing: BarItem,
}

impl BarItems {
    pub fn add_all(instance: Instance, config: Config) -> BarItems {
        let _typing = BarItem::new("discord_typing", move |_: &Weechat, buffer: &Buffer| {
            if let Some(channel_id) = buffer.channel_id() {
                let guild_id = buffer.guild_id();

                match config.typing_list_style() {
                    0 => terse_typing_list(
                        &instance,
                        channel_id,
                        guild_id,
                        config.typing_list_max() as usize,
                    ),
                    1 => expanded_typing_list(
                        &instance,
                        channel_id,
                        guild_id,
                        config.typing_list_max() as usize,
                    ),
                    _ => unreachable!(),
                }
            } else {
                "".into()
            }
        })
        .expect("Unable to add typing bar item");

        BarItems { _typing }
    }
}

fn terse_typing_list(
    instance: &Instance,
    channel_id: ChannelId,
    guild_id: Option<GuildId>,
    max_names: usize,
) -> String {
    let (head, has_more) = get_users_for_typing_list(
        &instance.borrow_typing_tracker_mut(),
        channel_id,
        guild_id,
        max_names,
    );

    let mut users = head.join(", ");
    if has_more {
        users = users + ", ...";
    }
    if users.is_empty() {
        "".into()
    } else {
        format!("typing: {}", users)
    }
}

fn expanded_typing_list(
    instance: &Instance,
    channel_id: ChannelId,
    guild_id: Option<GuildId>,
    max_names: usize,
) -> String {
    let (head, has_more) = get_users_for_typing_list(
        &instance.borrow_typing_tracker_mut(),
        channel_id,
        guild_id,
        max_names,
    );

    if head.is_empty() {
        "".into()
    } else if has_more {
        "Several people are typing...".into()
    } else if head.len() == 1 {
        format!("{} is typing", head[0])
    } else {
        let prefix = &head[..head.len() - 1];
        format!(
            "{} and {} are typing",
            prefix.join(", "),
            head[head.len() - 1]
        )
    }
}

fn get_users_for_typing_list(
    typing_tracker: &TypingTracker,
    channel_id: ChannelId,
    guild_id: Option<GuildId>,
    max_names: usize,
) -> (Vec<String>, bool) {
    let mut users = typing_tracker.typing(guild_id, channel_id);
    users.dedup();
    let (head, has_more) = if users.len() > max_names {
        (&users[..max_names], true)
    } else {
        (&users[..], false)
    };
    (head.to_vec(), has_more)
}
