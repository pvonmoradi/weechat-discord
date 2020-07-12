use crate::{
    config::Config, discord::discord_connection::DiscordConnection, guild_buffer::DiscordGuild,
    twilight_utils::ext::ChannelExt, DiscordSession,
};
use clap::{App, AppSettings, Arg, ArgMatches};
use std::sync::Arc;
use twilight::{cache::twilight_cache_inmemory::model::CachedGuild, model::channel::GuildChannel};
use weechat::{
    buffer::Buffer,
    hooks::{Command, CommandSettings},
    ArgsWeechat, Weechat,
};

pub struct DiscordCommand {
    session: DiscordSession,
    connection: DiscordConnection,
    config: Config,
}

impl DiscordCommand {
    fn add_guild(&self, matches: &ArgMatches) {
        // TODO: Abstract guild resolution code
        let cache = match self.connection.borrow().as_ref() {
            Some(conn) => conn.cache.clone(),
            None => {
                Weechat::print("discord: Discord must be connected to add servers");
                return;
            },
        };
        let guild_name = matches
            .value_of("name")
            .expect("name is required by verification")
            .to_string();

        {
            let config = self.config.clone();
            let session = self.session.clone();
            Weechat::spawn(async move {
                match crate::twilight_utils::search_cached_striped_guild_name(&cache, &guild_name)
                    .await
                {
                    Some(guild) => {
                        let mut config_borrow = config.borrow_mut();
                        let mut section = config_borrow
                            .search_section_mut("server")
                            .expect("Can't get server section");

                        if !session.guilds.borrow().contains_key(&guild.id) {
                            tracing::info!(%guild.id, %guild.name, "Adding guild to config.");
                            Weechat::print(&format!("discord: Added \"{}\"", guild.name));
                            session.guilds.borrow_mut().insert(
                                guild.id,
                                DiscordGuild::new(&config, guild.id, &mut section),
                            );
                        } else {
                            tracing::info!(%guild.id, %guild.name, "Guild not added to config, already exists.");
                            Weechat::print(&format!("\"{}\" has already been added", guild.name));
                        }
                        return;
                    },

                    None => {
                        tracing::info!("Could not find guild: \"{}\"", guild_name);
                        Weechat::print(&format!("Could not find guild: {}", guild_name));
                    },
                };
            });
        }
    }

    fn remove_guild(&self, matches: &ArgMatches) {
        let cache = match self.connection.borrow().as_ref() {
            Some(conn) => conn.cache.clone(),
            None => {
                Weechat::print("discord: Discord must be connected to remove servers");
                return;
            },
        };
        let guild_name = matches
            .value_of("name")
            .expect("name is required by verification")
            .to_string();

        {
            let session = self.session.clone();
            Weechat::spawn(async move {
                let guild_ids = session.guilds.borrow().keys().copied().collect::<Vec<_>>();
                match crate::twilight_utils::search_striped_guild_name(
                    &cache,
                    guild_ids,
                    &guild_name,
                )
                .await
                {
                    Some(guild) => {
                        if session.guilds.borrow_mut().remove(&guild.id).is_some() {
                            tracing::info!(%guild.id, %guild.name, "Removed guild from config.");
                            Weechat::print(&format!("discord: Removed \"{}\"", guild.name));
                        } else {
                            tracing::info!(%guild.id, %guild.name, "Guild not added.");
                            Weechat::print(&format!(
                                "discord: Server \"{}\" not in config",
                                guild.name
                            ));
                        }
                    },
                    None => {
                        tracing::info!("Could not find guild: \"{}\"", guild_name);
                        Weechat::print(&format!("Could not find guild: {}", guild_name));
                    },
                };
            });
        }
    }

    fn list_guilds(&self) {
        Weechat::print("discord: Servers:");

        if let Some(connection) = self.connection.borrow().as_ref() {
            let cache = connection.cache.clone();
            for (guild_id, guild_) in self.session.guilds.borrow().clone().into_iter() {
                let cache = cache.clone();
                Weechat::spawn(async move {
                    let guild = cache
                        .guild(guild_id)
                        .await
                        .expect("InMemoryCache cannot fail");
                    if let Some(guild) = guild {
                        Weechat::print(&format!("{}{}", Weechat::color("chat_server"), guild.name));
                    } else {
                        Weechat::print(&format!("{:?}", guild_id));
                    }

                    for channel_id in guild_.autojoin().iter() {
                        if let Some(channel) = cache
                            .guild_channel(*channel_id)
                            .await
                            .expect("InMemoryCache cannot fail")
                        {
                            Weechat::print(&format!("  #{}", channel.name()));
                        } else {
                            Weechat::print(&format!("  #{:?}", channel_id));
                        }
                    }
                });
            }
        } else {
            for (guild_id, guild) in self.session.guilds.borrow().clone().into_iter() {
                Weechat::print(&format!("{:?}", guild_id));
                for channel_id in guild.autojoin() {
                    Weechat::print(&format!("  #{:?}", channel_id));
                }
            }
        }
    }

    fn autoconnect_guild(&self, matches: &ArgMatches) {
        let guild_name = matches
            .value_of("name")
            .expect("name is required by verification")
            .to_string();

        let session = self.session.clone();
        let connection = self.connection.clone();
        Weechat::spawn(async move {
            let conn = connection.borrow();
            let conn = match conn.as_ref() {
                Some(conn) => conn,
                None => {
                    Weechat::print(
                        "discord: Discord must be connected to enable server autoconnect",
                    );
                    return;
                },
            };

            match crate::twilight_utils::search_striped_guild_name(
                &conn.cache,
                session.guilds.borrow().keys().copied(),
                &guild_name,
            )
            .await
            {
                Some(guild) => {
                    if let Some(weechat_guild) = session.guilds.borrow().get(&guild.id) {
                        tracing::info!(%guild.id, %guild.name, "Enabled autoconnect for guild");
                        weechat_guild.set_autoconnect(true);
                        weechat_guild.write_config();
                        Weechat::print(&format!(
                            "discord: Now autoconnecting to \"{}\"",
                            guild.name
                        ));
                        let _ = weechat_guild.connect(&conn, session.guilds.clone()).await;
                    } else {
                        tracing::info!(%guild.id, %guild.name, "Guild not added.");
                        Weechat::print(&format!(
                            "discord: Server \"{}\" not in config",
                            guild.name
                        ));
                    }
                },
                None => {
                    tracing::info!("Could not find guild: \"{}\"", guild_name);
                    Weechat::print(&format!("Could not find guild: {}", guild_name));
                },
            };
        });
    }

    fn noautoconnect_guild(&self, matches: &ArgMatches) {
        let guild_name = matches
            .value_of("name")
            .expect("name is required by verification")
            .to_string();

        let session = self.session.clone();
        let connection = self.connection.clone();
        Weechat::spawn(async move {
            let cache = match connection.borrow().as_ref() {
                Some(conn) => conn.cache.clone(),
                None => {
                    Weechat::print(
                        "discord: Discord must be connected to enable server autoconnect",
                    );
                    return;
                },
            };

            match crate::twilight_utils::search_striped_guild_name(
                &cache,
                session.guilds.borrow().keys().copied(),
                &guild_name,
            )
            .await
            {
                Some(guild) => {
                    if let Some(weechat_guild) = session.guilds.borrow().get(&guild.id) {
                        tracing::info!(%guild.id, %guild.name, "Disabled autoconnect for guild");
                        weechat_guild.set_autoconnect(false);
                        weechat_guild.write_config();
                        Weechat::print(&format!(
                            "discord: No longer autoconnecting to \"{}\"",
                            guild.name
                        ));
                    } else {
                        tracing::info!(%guild.id, %guild.name, "Guild not added.");
                        Weechat::print(&format!(
                            "discord: Server \"{}\" not in config",
                            guild.name
                        ));
                    }
                },
                None => {
                    tracing::info!("Could not find guild: \"{}\"", guild_name);
                    Weechat::print(&format!("Could not find guild: {}", guild_name));
                },
            };
        });
    }

    fn process_server_matches(&self, matches: &ArgMatches) {
        match matches.subcommand() {
            ("add", Some(matches)) => self.add_guild(matches),
            ("remove", Some(matches)) => self.remove_guild(matches),
            ("list", _) => self.list_guilds(),
            ("autoconnect", Some(matches)) => self.autoconnect_guild(matches),
            ("noautoconnect", Some(matches)) => self.noautoconnect_guild(matches),
            _ => unreachable!("Reached subcommand that does not exist in clap config"),
        }
    }

    fn add_autojoin_channel(&self, matches: &ArgMatches) {
        if let Some((guild, weecord_guild, channel)) = self.resolve_channel_and_guild(matches) {
            weecord_guild.autojoin_mut().push(channel.id());
            weecord_guild.write_config();
            tracing::info!(%weecord_guild.id, channel.id=%channel.id(), "Added channel to autojoin list");
            Weechat::print(&format!("Added channel {} to autojoin list", guild.name));

            let connection = self.connection.clone();
            Weechat::spawn(async move {
                let conn = connection.borrow();
                let conn = match conn.as_ref() {
                    Some(conn) => conn,
                    None => {
                        Weechat::print("discord: Discord must be connected to join channels");
                        return;
                    },
                };

                weecord_guild.join_channel(conn, &guild, &channel).await;
            });
        }
    }

    fn remove_autojoin_channel(&self, matches: &ArgMatches) {
        if let Some((guild, weecord_guild, channel)) = self.resolve_channel_and_guild(matches) {
            {
                // TODO: Vec::remove_item when it stabilizes
                let mut autojoin = weecord_guild.autojoin_mut();
                if let Some(pos) = autojoin.iter().position(|x| *x == channel.id()) {
                    autojoin.remove(pos);
                    tracing::info!(%weecord_guild.id, channel.id=%channel.id(), "Removed channel from autojoin list");
                    Weechat::print(&format!(
                        "Removed channel {} from autojoin list",
                        guild.name
                    ));
                }
            }
            weecord_guild.write_config();
        }
    }

    fn join_channel(&self, matches: &ArgMatches) {
        if let Some((guild, weecord_guild, channel)) = self.resolve_channel_and_guild(matches) {
            let connection = self.connection.clone();
            Weechat::spawn(async move {
                let conn = connection.borrow();
                let conn = match conn.as_ref() {
                    Some(conn) => conn,
                    None => {
                        Weechat::print("discord: Discord must be connected to join channels");
                        return;
                    },
                };

                weecord_guild.join_channel(conn, &guild, &channel).await;
            });
        }
    }

    fn resolve_channel_and_guild(
        &self,
        matches: &ArgMatches,
    ) -> Option<(Arc<CachedGuild>, DiscordGuild, Arc<GuildChannel>)> {
        let guild_name = matches
            .value_of("guild_name")
            .expect("guild name is required by verification")
            .to_string();
        let channel_name = matches
            .value_of("name")
            .expect("channel name is required by verification")
            .to_string();

        let connection = self.connection.borrow();
        let connection = match connection.as_ref() {
            Some(conn) => conn,
            None => {
                Weechat::print("discord: Discord must be connected to join channels");
                return None;
            },
        };

        let guilds = self.session.guilds.clone();
        let cache = connection.cache.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        connection.rt.spawn(async move {
            if let Some(guild) =
                crate::twilight_utils::search_cached_striped_guild_name(&cache, &guild_name).await
            {
                tracing::trace!(%guild.name, "Matched guild");
                if let Some(channel) =
                    crate::twilight_utils::search_cached_stripped_guild_channel_name(
                        &cache,
                        guild.id,
                        &channel_name,
                    )
                    .await
                {
                    tracing::trace!("Matched channel {}", channel.name());
                    tx.send((guild, channel)).expect("main thread panicked?");
                } else {
                    tracing::warn!(%channel_name, "Unable to find matching channel");
                    Weechat::spawn_from_thread(async move {
                        Weechat::print(&format!("Could not find channel: {}", channel_name));
                    });
                }
            } else {
                tracing::warn!(%channel_name, "Unable to find matching guild");
                Weechat::spawn_from_thread(async move {
                    Weechat::print(&format!("Could not find server: {}", guild_name));
                });
            }
        });

        if let Ok((guild, channel)) = rx.recv() {
            if let Some(weecord_guild) = guilds.borrow().values().find(|g| g.id == guild.id) {
                Some((guild, weecord_guild.clone(), channel))
            } else {
                tracing::warn!(%guild.id, "Guild has not been added to weechat");
                Weechat::spawn_from_thread(async move {
                    Weechat::print(&format!("Could not find server in config: {}", guild.name));
                });
                None
            }
        } else {
            None
        }
    }

    fn process_channel_matches(&self, matches: &ArgMatches) {
        match matches.subcommand() {
            ("autojoin", Some(matches)) => self.add_autojoin_channel(matches),
            ("noautojoin", Some(matches)) => self.remove_autojoin_channel(matches),
            ("join", Some(matches)) => self.join_channel(matches),
            _ => {},
        }
    }

    fn token(&self, matches: &ArgMatches) {
        let token = matches.value_of("token").expect("required by validation");

        self.config.borrow_inner_mut().token = Some(token.trim().trim_matches('"').to_string());
        self.config.write();

        Weechat::print("discord: Updated Discord token");
        tracing::info!("updated discord token");
    }

    fn process_debug_matches(&self, matches: &ArgMatches) {
        match matches.subcommand() {
            ("buffer", Some(_)) => {
                for guild in self.session.guilds.guilds.borrow().values() {
                    let (strng, weak) = guild.debug_counts();
                    Weechat::print(&format!("Guild [{} {}]: {}", strng, weak, guild.id));

                    for channel in guild.channel_buffers().values() {
                        Weechat::print(&format!("  Channel: {}", channel.id));
                    }
                }
            },
            _ => {},
        }
    }
}

impl weechat::hooks::CommandCallback for DiscordCommand {
    fn callback(&mut self, _: &Weechat, _: &Buffer, arguments: ArgsWeechat) {
        let args = arguments.collect::<Vec<_>>();

        let app = App::new("/discord")
            .global_setting(AppSettings::DisableVersion)
            .global_setting(AppSettings::VersionlessSubcommands)
            .setting(AppSettings::SubcommandRequiredElseHelp)
            .subcommand(
                App::new("server")
                    .setting(AppSettings::SubcommandRequiredElseHelp)
                    .subcommand(App::new("add").arg(Arg::with_name("name").required(true)))
                    .subcommand(
                        App::new("remove")
                            .arg(Arg::with_name("name").required(true))
                            .alias("rm"),
                    )
                    .subcommand(App::new("autoconnect").arg(Arg::with_name("name").required(true)))
                    .subcommand(
                        App::new("noautoconnect").arg(Arg::with_name("name").required(true)),
                    )
                    .subcommand(App::new("list")),
            )
            .subcommand(
                App::new("channel")
                    .setting(AppSettings::SubcommandRequiredElseHelp)
                    .subcommand(
                        App::new("autojoin")
                            .arg(Arg::with_name("guild_name").required(true))
                            .arg(Arg::with_name("name").required(true)),
                    )
                    .subcommand(
                        App::new("noautojoin")
                            .arg(Arg::with_name("guild_name").required(true))
                            .arg(Arg::with_name("name").required(true)),
                    )
                    .subcommand(
                        App::new("join")
                            .arg(Arg::with_name("guild_name").required(true))
                            .arg(Arg::with_name("name").required(true)),
                    ),
            )
            .subcommand(
                App::new("debug")
                    .setting(AppSettings::SubcommandRequiredElseHelp)
                    .subcommand(App::new("buffer")),
            )
            .subcommand(App::new("token").arg(Arg::with_name("token").required(true)));

        let matches = match app.try_get_matches_from(args) {
            Ok(m) => {
                tracing::trace!("{:#?}", m);
                m
            },
            Err(e) => {
                tracing::trace!("{:#?}", e);
                Weechat::print(
                    &Weechat::execute_modifier("color_decode_ansi", "1", &e.to_string()).unwrap(),
                );
                return;
            },
        };

        match matches.subcommand() {
            ("server", Some(matches)) => self.process_server_matches(matches),
            ("channel", Some(matches)) => self.process_channel_matches(matches),
            ("token", Some(matches)) => self.token(matches),
            ("debug", Some(matches)) => self.process_debug_matches(matches),
            _ => {},
        };
    }
}

pub fn hook(
    weechat: &Weechat,
    connection: DiscordConnection,
    session: DiscordSession,
    config: Config,
) -> Command {
    weechat.hook_command(
        CommandSettings::new("discord")
            .description("Discord integration for weechat")
            .add_argument("token <token>")
            .add_argument("server add|remove|list|autoconnect|noautoconnect <server-name>")
            .add_argument("channel join|autojoin|noautojoin <server-name> <channel-name>")
            .add_completion("token")
            .add_completion("server add|remove|list|autoconnect|noautoconnect %(discord_guild)")
            .add_completion("channel join|autojoin|noautojoin %(discord_guild) %(discord_channel)")
            .add_completion("debug buffer"),
        DiscordCommand {
            session,
            connection,
            config,
        },
    )
}
