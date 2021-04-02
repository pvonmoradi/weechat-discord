use crate::{
    on_main,
    sync::on_main_blocking,
    utils,
    utils::{BufferExt, ChannelExt},
    weechat_utils::{BufferManager, MessageManager},
    Discord,
};
use indexmap::IndexMap;
use serenity::{
    cache::{Cache, CacheRwLock},
    client::bridge::gateway,
    constants::OpCode,
    model::prelude::*,
    prelude::*,
};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::Arc,
};
use weechat::{buffer::HotlistPriority, Buffer, ConfigOption, NickArgs, Weechat};

const OFFLINE_GROUP_NAME: &str = "99999|Offline";
const ONLINE_GROUP_NAME: &str = "99998|Online";
const BOT_GROUP_NAME: &str = "99997|Bot";

pub fn init(weechat: &Weechat) -> BufferManager {
    BufferManager::new(Weechat::from_ptr(weechat.as_ptr()))
}

pub fn create_buffers(ready_data: &Ready) {
    let ctx = match crate::discord::get_ctx() {
        Some(ctx) => ctx,
        _ => return,
    };
    let current_user = ctx.cache.read().user.clone();

    let guilds = match current_user.guilds(ctx) {
        Ok(guilds) => guilds,
        Err(e) => {
            crate::plugin_print(&format!("Error getting user guilds: {:?}", e));
            vec![]
        },
    };
    let mut map: HashMap<_, _> = guilds.iter().map(|g| (g.id, g)).collect();

    let mut sorted_guilds = VecDeque::new();

    // Add the guilds ordered from the client
    for guild_id in &ready_data.user_settings.guild_positions {
        if let Some(guild) = map.remove(&guild_id) {
            sorted_guilds.push_back(guild);
        }
    }

    // Prepend any remaning guilds
    for guild in map.values() {
        sorted_guilds.push_front(guild);
    }

    for guild in &sorted_guilds {
        let guild_settings = ready_data.user_guild_settings.get(&guild.id.into());
        let guild_muted;
        let mut channel_muted = HashMap::new();
        if let Some(guild_settings) = guild_settings {
            guild_muted = guild_settings.muted;
            for (channel_id, channel_override) in &guild_settings.channel_overrides {
                channel_muted.insert(channel_id, channel_override.muted);
            }
        } else {
            guild_muted = false;
        }
        create_guild_buffer(guild.id, &guild.name);

        // TODO: Colors?
        let nick = if let Ok(current_member) = guild.id.member(ctx, current_user.id) {
            format!("@{}", current_member.display_name())
        } else {
            format!("@{}", current_user.name)
        };
        let channels = guild.id.channels(ctx).expect("Unable to fetch channels");
        let mut channels = channels.values().collect::<Vec<_>>();
        channels.sort_by_key(|g| g.position);
        for channel in channels {
            let is_muted =
                guild_muted || channel_muted.get(&channel.id).cloned().unwrap_or_default();
            create_buffer_from_channel(&ctx.cache, &guild.name, &channel, &nick, is_muted);
        }
    }
}

// TODO: Merge these functions
pub fn create_autojoin_buffers(_ready: &Ready) {
    let ctx = match crate::discord::get_ctx() {
        Some(ctx) => ctx,
        _ => return,
    };

    let current_user = ctx.cache.read().user.clone();

    // TODO: Add sorting
    let mut autojoin_items: Vec<_> = on_main_blocking(|weecord| weecord.config.autojoin_channels());

    let watched_items: Vec<_> = on_main_blocking(|weecord| weecord.config.watched_channels());

    let watched_channels = utils::flatten_guilds(&ctx, &watched_items);

    let cache = ctx.cache.read();
    for (guild_id, channels) in watched_channels {
        for channel in channels {
            let read_state = match cache.read_state.get(&channel) {
                Some(rs) => rs,
                None => continue,
            };
            let last_msg = match channel
                .to_channel_cached(ctx)
                .and_then(|c| c.last_message())
            {
                Some(msg) => msg,
                None => continue,
            };

            if read_state.last_message_id != last_msg {
                autojoin_items.push(utils::GuildOrChannel::Channel(guild_id, channel))
            }
        }
    }

    // flatten guilds into channels
    let autojoin_channels = utils::flatten_guilds(&ctx, &autojoin_items);

    create_buffers_from_flat_items(&ctx, &current_user, &autojoin_channels);
}

pub fn create_buffers_from_flat_items(
    ctx: &Context,
    current_user: &CurrentUser,
    channels: &IndexMap<Option<GuildId>, Vec<ChannelId>>,
) {
    // TODO: Flatten and iterate by guild, then channel
    for guild_id in channels.iter() {
        match guild_id {
            (Some(guild_id), channels) => {
                let guild = match guild_id.to_guild_cached(&ctx.cache) {
                    Some(guild) => guild,
                    None => continue,
                };
                let guild = guild.read();

                // TODO: Colors?
                let nick = if let Ok(current_member) = guild.id.member(ctx, current_user.id) {
                    format!("@{}", current_member.display_name())
                } else {
                    format!("@{}", current_user.name)
                };
                let nick = &nick;

                create_guild_buffer(guild.id, &guild.name);

                parking_lot::RwLockReadGuard::unlock_fair(guild);

                for channel in channels {
                    // TODO: Muting
                    let () = on_main_blocking(move |_| {
                        let ctx = match crate::discord::get_ctx() {
                            Some(ctx) => ctx,
                            _ => return,
                        };

                        let guild = match guild_id.to_guild_cached(&ctx.cache) {
                            Some(guild) => guild,
                            None => return,
                        };
                        let guild = guild.read();

                        let channel = match channel
                            .to_channel_cached(&ctx.cache)
                            .and_then(Channel::guild)
                        {
                            Some(channel) => channel,
                            None => return,
                        };

                        create_buffer_from_channel(
                            &ctx.cache,
                            &guild.name,
                            &channel.read(),
                            &nick,
                            false,
                        );
                    });
                }
            },
            (None, channels) => {
                let ctx = match crate::discord::get_ctx() {
                    Some(ctx) => ctx,
                    _ => return,
                };
                let cache = ctx.cache.read();
                let nick = cache.user.name.to_string();

                for channel_id in channels {
                    let nick = format!("@{}", nick);
                    let channel = if let Ok(channel) = channel_id.to_channel(ctx) {
                        channel
                    } else {
                        crate::plugin_print("cache miss");
                        continue;
                    };

                    match channel {
                        channel @ Channel::Private(_) => on_main(move |weecord| {
                            let ctx = match crate::discord::get_ctx() {
                                Some(ctx) => ctx,
                                _ => return,
                            };
                            create_buffer_from_dm(&ctx.cache, weecord, channel, &nick, false);
                        }),

                        channel @ Channel::Group(_) => on_main(move |weecord| {
                            let ctx = match crate::discord::get_ctx() {
                                Some(ctx) => ctx,
                                _ => return,
                            };
                            create_buffer_from_group(&ctx.cache, weecord, channel, &nick);
                        }),
                        _ => unreachable!(),
                    }
                }
            },
        }
    }
}

fn user_online(cache: &Cache, user_id: UserId) -> bool {
    if user_id == cache.user.id {
        true
    } else {
        let presence = cache.presences.get(&user_id);
        presence
            .map(|p| utils::status_is_online(p.status))
            .unwrap_or(false)
    }
}

pub fn create_guild_buffer(id: GuildId, name: &str) {
    let guild_name_id = utils::buffer_id_for_guild(id);
    let () = on_main_blocking(move |weecord| {
        let buffer = weecord.buffer_manager.get_or_create_buffer(&guild_name_id);

        buffer.set_localvar("guild_name", name);
        buffer.set_localvar("server", name);
        buffer.set_short_name(name);
        buffer.set_localvar("guildid", &id.0.to_string());
        buffer.set_localvar("type", "server");
    });
}

pub fn create_buffer_from_channel(
    cache: &CacheRwLock,
    guild_name: &str,
    channel: &GuildChannel,
    nick: &str,
    muted: bool,
) {
    let current_user = cache.read().user.clone();
    if let Ok(perms) = channel.permissions_for_user(cache, current_user.id) {
        if !perms.read_message_history() {
            return;
        }
    }

    let channel_type = match channel.kind {
        // TODO: Should we display store channels somehow?
        ChannelType::Category | ChannelType::Voice | ChannelType::Store | ChannelType::Stage => {
            return
        },
        ChannelType::Private => "private",
        ChannelType::Group | ChannelType::Text | ChannelType::News => "channel",
        ChannelType::__Nonexhaustive => unreachable!(),
    };

    let name_id = utils::buffer_id_for_channel(Some(channel.guild_id), channel.id);
    let has_unread = cache
        .read()
        .read_state
        .get(&channel.id)
        .map(|rs| rs.last_message_id)
        != channel.last_message_id;

    let () = on_main_blocking(|weecord| {
        let buffer = weecord.buffer_manager.get_or_create_buffer(&name_id);

        buffer.set_short_name(&channel.name);

        buffer.set_localvar("channelid", &channel.id.0.to_string());
        buffer.set_localvar("guildid", &channel.guild_id.0.to_string());
        buffer.set_localvar("channel", &channel.name);
        buffer.set_localvar("guild_name", guild_name);
        buffer.set_localvar("server", guild_name);
        buffer.set_localvar("type", channel_type);
        buffer.set_localvar("nick", &nick);
        if has_unread && !muted {
            buffer.set_hotlist(HotlistPriority::Message);
        }

        let mut title = if let Some(ref topic) = channel.topic {
            if !topic.is_empty() {
                format!("{} | {}", channel.name, topic)
            } else {
                channel.name.clone()
            }
        } else {
            channel.name.clone()
        };

        if muted {
            title += " (muted)";
        }
        buffer.set_title(&title);
        buffer.set_localvar("muted", &(muted as u8).to_string());
    });
}

// TODO: Reduce code duplication
pub fn create_buffer_from_dm(
    cache: &CacheRwLock,
    weecord: &crate::Discord,
    channel: Channel,
    nick: &str,
    switch_to: bool,
) {
    let channel = match channel.private() {
        Some(chan) => chan,
        None => return,
    };
    let channel = channel.read();

    let name_id = utils::buffer_id_for_channel(None, channel.id);
    let buffer = weecord.buffer_manager.get_or_create_buffer(&name_id);

    buffer.set_short_name(&channel.name());
    buffer.set_localvar("channelid", &channel.id.0.to_string());
    buffer.set_localvar("nick", &nick);

    let has_unread = cache
        .read()
        .read_state
        .get(&channel.id)
        .map(|rs| rs.last_message_id)
        != channel.last_message_id;

    if has_unread {
        buffer.set_hotlist(HotlistPriority::Private);
    }

    if switch_to {
        buffer.switch_to();
    }
    let title = format!("DM with {}", channel.recipient.read().name);
    buffer.set_title(&title);

    load_dm_nicks(&buffer, &*channel);
}

pub fn create_buffer_from_group(
    cache: &CacheRwLock,
    weecord: &Discord,
    channel: Channel,
    nick: &str,
) {
    let channel = match channel.group() {
        Some(chan) => chan,
        None => return,
    };
    let channel = channel.read();

    let title = format!(
        "DM with {}",
        channel
            .recipients
            .values()
            .map(|u| u.read().name.to_owned())
            .collect::<Vec<_>>()
            .join(", ")
    );

    let name_id = utils::buffer_id_for_channel(None, channel.channel_id);

    let buffer = weecord.buffer_manager.get_or_create_buffer(&name_id);

    buffer.set_short_name(&channel.name());
    buffer.set_localvar("channelid", &channel.channel_id.0.to_string());
    buffer.set_localvar("nick", &nick);
    buffer.set_title(&title);

    let has_unread = cache
        .read()
        .read_state
        .get(&channel.channel_id)
        .map(|rs| rs.last_message_id)
        != channel.last_message_id;

    if has_unread {
        buffer.set_hotlist(HotlistPriority::Private);
    }
}

pub fn create_pins_buffer(weecord: &Discord, channel: &Channel) {
    let buffer_name = format!("Pins.{}", channel.id().0);

    let buffer = weecord.buffer_manager.get_or_create_buffer(&buffer_name);
    buffer.switch_to();

    buffer.set_title(&format!("Pinned messages in #{}", channel.name()));
    buffer.set_full_name(&format!("Pinned messages in ${}", channel.name()));
    buffer.set_short_name(&format!("#{} pins", channel.name()));
    utils::set_pins_for_channel(&buffer, channel.id());
}

pub fn load_pin_buffer_history(buffer: &MessageManager) {
    let channel = match utils::pins_for_channel(&buffer) {
        Some(ch) => ch,
        None => return,
    };

    buffer.set_history_loaded();
    buffer.clear();
    let buffer_name = buffer.get_name().to_string();

    std::thread::spawn(move || {
        let ctx = match crate::discord::get_ctx() {
            Some(ctx) => ctx,
            _ => return,
        };

        let pins = match channel.pins(ctx) {
            Ok(pins) => pins,
            Err(_) => return,
        };

        on_main(move |weecord| {
            let ctx = match crate::discord::get_ctx() {
                Some(ctx) => ctx,
                _ => return,
            };
            let buf = match weecord.buffer_manager.get_buffer(&buffer_name) {
                Some(buf) => buf,
                None => return,
            };

            for pin in pins.iter().rev() {
                buf.add_message(&ctx.cache, pin, false);
            }
        });
    });
}

pub fn load_pin_buffer_history_for_id(id: ChannelId) {
    on_main(move |weecord| {
        if let Some(buffer) = weecord.buffer_manager.get_buffer(&format!("Pins.{}", id)) {
            load_pin_buffer_history(&buffer)
        };
    })
}

pub fn load_history(
    buffer: &MessageManager,
    completion_sender: crossbeam_channel::Sender<()>,
    fetch_count: i32,
) {
    let channel = if let Some(channel) = buffer.channel_id() {
        channel
    } else {
        return;
    };
    let guild = buffer.guild_id();

    buffer.clear();
    buffer.set_history_loaded();

    let buffer_name = buffer.get_name().to_string();

    std::thread::spawn(move || {
        let ctx = match crate::discord::get_ctx() {
            Some(ctx) => ctx,
            _ => return,
        };

        if let Ok(msgs) = channel.messages(ctx, |retriever| retriever.limit(fetch_count as u64)) {
            on_main(move |weechat| {
                let ctx = match crate::discord::get_ctx() {
                    Some(ctx) => ctx,
                    _ => return,
                };
                let mut unknown_users = HashSet::new();
                let buf = match weechat.buffer_manager.get_buffer(&buffer_name) {
                    Some(buf) => buf,
                    None => return,
                };

                if let Some(read_state) = ctx.cache.read().read_state.get(&channel) {
                    let unread_in_page = msgs.iter().any(|m| m.id == read_state.last_message_id);

                    if unread_in_page {
                        let mut backlog = true;
                        for msg in msgs.into_iter().rev() {
                            unknown_users.extend(buf.add_message(&ctx.cache, &msg, false));

                            if backlog {
                                buf.mark_read();
                                buf.clear_hotlist();
                            }
                            if msg.id == read_state.last_message_id {
                                backlog = false;
                            }
                        }
                    } else {
                        buf.mark_read();
                        buf.clear_hotlist();
                        for msg in msgs.into_iter().rev() {
                            unknown_users.extend(buf.add_message(&ctx.cache, &msg, false));
                        }
                    }
                } else {
                    for msg in msgs.into_iter().rev() {
                        unknown_users.extend(buf.add_message(&ctx.cache, &msg, false));
                    }
                }
                if let Some(guild) = guild {
                    let msg = json::object! {
                        "op" =>  OpCode::GetGuildMembers.num(),
                        "d" => json::object! {
                            "guild_id" => guild.0.to_string(),
                            "user_ids" => (unknown_users.iter().map(|id| id.to_string())).collect::<Vec<_>>(),
                            "nonce" => channel.0.to_string(),
                        }
                    };
                    ctx.shard
                        .websocket_message(gateway::Message::Text(msg.to_string()));
                }
                let _ = completion_sender.send(());
            });
        }
    });
}

pub fn load_dm_nicks(buffer: &MessageManager, channel: &PrivateChannel) {
    let weechat = buffer.get_weechat();
    let use_presence = crate::upgrade_plugin(&weechat).config.use_presence.value();

    // If the user doesn't want the presence, there's no reason to open
    // the nicklist
    if use_presence {
        buffer.set_nicks_loaded();
        buffer.enable_nicklist();

        let ctx = match crate::discord::get_ctx() {
            Some(ctx) => ctx,
            _ => return,
        };

        let recip = channel.recipient.read();
        let cache = ctx.cache.read();

        buffer.add_nick(
            NickArgs {
                name: &recip.name,
                color: &utils::nick_color(&weechat, &recip.name),
                prefix: &utils::get_user_status_prefix(&weechat, &cache, recip.id),
                ..Default::default()
            },
            None,
        );

        // TODO: Detect current user status properly
        buffer.add_nick(
            NickArgs {
                name: &cache.user.name,
                color: &utils::nick_color(&weechat, &cache.user.name),
                prefix: &utils::format_user_status_prefix(
                    &weechat,
                    Some(*crate::command::LAST_STATUS.lock()),
                ),
                ..Default::default()
            },
            None,
        );
    }
}

// TODO: Make this nicer somehow
// TODO: Refactor this to use `?`
pub fn load_nicks(buffer: &Buffer) {
    if buffer.nicks_loaded() {
        return;
    }

    let guild_id = if let Some(guild_id) = buffer.guild_id() {
        guild_id
    } else {
        return;
    };

    let channel_id = if let Some(channel_id) = buffer.channel_id() {
        channel_id
    } else {
        return;
    };

    buffer.set_nicks_loaded();
    buffer.enable_nicklist();

    let sealed_buffer = buffer.seal();

    std::thread::spawn(move || {
        let ctx = match crate::discord::get_ctx() {
            Some(ctx) => ctx,
            _ => return,
        };

        let guild = guild_id.to_guild_cached(ctx).expect("No guild cache item");

        // TODO: What to do with more than 1000 members?
        // NOTE: using `guild.read().members` 403s and invalidates a users verification status
        let members: Vec<_> = guild.read().members.values().cloned().collect();

        drop(guild);

        let () = on_main_blocking(move |weechat| {
            let ctx = match crate::discord::get_ctx() {
                Some(ctx) => ctx,
                _ => return,
            };

            let use_presence = weechat.config.use_presence.value();

            let buffer = sealed_buffer.unseal(&weechat);
            let guild = guild_id.to_guild_cached(ctx).expect("No guild cache item");

            let has_crown = guild_has_crown(&guild.read());

            for member in members {
                add_member_to_nicklist(
                    weechat,
                    &ctx,
                    &buffer,
                    channel_id,
                    &guild,
                    &member,
                    use_presence,
                    has_crown,
                );
            }
        });
    });
}

fn add_member_to_nicklist(
    weechat: &Weechat,
    ctx: &Context,
    buffer: &Buffer,
    channel_id: ChannelId,
    guild: &Arc<RwLock<Guild>>,
    member: &Member,
    use_presence: bool,
    guild_has_crown: bool,
) {
    let user = member.user.read();
    // the current user does not seem to usually have a presence, assume they are online
    let online = if use_presence {
        user_online(&*ctx.cache.read(), user.id)
    } else {
        false
    };

    let member_perms = guild.read().user_permissions_in(channel_id, user.id);
    // A pretty accurate method of checking if a user is "in" a channel
    if !member_perms.read_message_history() || !member_perms.read_messages() {
        return;
    }

    let role_name;
    let role_color;
    // TODO: Change offline/online color somehow?
    if user.bot {
        role_name = BOT_GROUP_NAME.to_owned();
        role_color = "gray".to_string();
    } else if !online && use_presence {
        role_name = OFFLINE_GROUP_NAME.to_owned();
        role_color = "grey".to_string();
    } else if let Some((highest_hoisted, highest)) = utils::find_highest_roles(&ctx.cache, &member)
    {
        role_name = format!(
            "{}|{}",
            99999 - highest_hoisted.position,
            highest_hoisted.name
        );
        role_color = crate::utils::rgb_to_ansi(highest.colour).to_string();
    } else {
        // Can't find a role, add user to generic bucket
        if use_presence {
            if online {
                role_name = ONLINE_GROUP_NAME.to_owned();
            } else {
                role_name = OFFLINE_GROUP_NAME.to_owned();
            }
            role_color = "grey".to_string();
        } else {
            buffer.add_nick(
                weechat::NickArgs {
                    name: member.display_name().as_ref(),
                    color: &utils::nick_color(&weechat, member.display_name().as_ref()),
                    ..Default::default()
                },
                None,
            );
            return;
        }
    }
    let group = match buffer.search_nicklist_group(&role_name) {
        Some(group) => group,
        None => buffer.add_group(&role_name, &role_color, true, None),
    };

    // TODO: Only show crown if there are no roles
    let nicklist_name = if guild_has_crown && guild.read().owner_id == user.id {
        format!("{} {}♛", member.display_name(), weechat.color("214"))
    } else {
        member.display_name().into_owned()
    };

    buffer.add_nick(
        weechat::NickArgs {
            name: nicklist_name.as_ref(),
            color: &utils::nick_color(&weechat, &nicklist_name),
            ..Default::default()
        },
        Some(&group),
    );
}

pub fn update_nick() {
    let ctx = match crate::discord::get_ctx() {
        Some(ctx) => ctx,
        _ => return,
    };
    let current_user = ctx.cache.read().user.clone();

    for guild in current_user.guilds(ctx).expect("Unable to fetch guilds") {
        // TODO: Colors?
        let nick = if let Ok(current_member) = guild.id.member(ctx, current_user.id) {
            format!("@{}", current_member.display_name())
        } else {
            format!("@{}", current_user.name)
        };

        let channels = guild.id.channels(ctx).expect("Unable to fetch channels");
        on_main(move |weechat| {
            for channel_id in channels.keys() {
                let string_channel = utils::buffer_id_for_channel(Some(guild.id), *channel_id);
                let nick = nick.to_owned();
                if let Some(buffer) = weechat.buffer_search("weecord", &string_channel) {
                    buffer.set_localvar("nick", &nick);
                    weechat.update_bar_item("input_prompt");
                }
            }
        })
    }
}

pub fn update_member_nick(old: &Option<Member>, new: &Member) {
    let old_nick = if let Some(old) = old.as_ref().map(Member::display_name) {
        old
    } else {
        // TODO: Rebuild entire nicklist?
        return;
    };
    let new_nick = new.display_name();
    let new = new.clone();
    let guild_id = new.guild_id;

    if old_nick != new_nick {
        let old_nick = old_nick.to_owned().to_string();
        let ctx = match crate::discord::get_ctx() {
            Some(ctx) => ctx,
            _ => return,
        };

        let channels = guild_id.channels(ctx).expect("Unable to fetch channels");

        on_main(move |weechat| {
            let ctx = match crate::discord::get_ctx() {
                Some(ctx) => ctx,
                _ => return,
            };
            for channel_id in channels.keys() {
                let string_channel = utils::buffer_id_for_channel(Some(guild_id), *channel_id);
                if let Some(buffer) = weechat.buffer_search("weecord", &string_channel) {
                    if let Some(nick) = buffer.search_nick(&old_nick, None) {
                        nick.remove();
                        if let Some(guild) = guild_id.to_guild_cached(&ctx) {
                            add_member_to_nicklist(
                                weechat,
                                &ctx,
                                &buffer,
                                *channel_id,
                                &guild,
                                &new,
                                false,
                                guild_has_crown(&guild.read()),
                            );
                        }
                    }
                }
            }
        })
    }
}

fn guild_has_crown(guild: &Guild) -> bool {
    for role in guild.roles.values() {
        if role.hoist && role.permissions.administrator() {
            return false;
        }
    }
    true
}
