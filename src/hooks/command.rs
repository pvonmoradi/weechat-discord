use crate::{
    config::Config, discord::discord_connection::DiscordConnection, guild_buffer::DiscordGuild,
    twilight_utils::ext::guild_channel_ext::GuildChannelExt, DiscordSession,
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
                match crate::twilight_utils::search_cached_striped_guild_name(
                    cache.as_ref(),
                    &guild_name,
                )
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
                match crate::twilight_utils::search_striped_guild_name(
                    cache.as_ref(),
                    session.guilds.borrow().keys().copied(),
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

    fn process_server_matches(&self, matches: &ArgMatches) {
        match matches.subcommand() {
            ("add", Some(matches)) => self.add_guild(matches),
            ("remove", Some(matches)) => self.remove_guild(matches),
            ("list", _) => self.list_guilds(),
            _ => unreachable!("Reached subcommand that does not exist in clap config"),
        }
    }

    fn add_autojoin_channel(&self, matches: &ArgMatches) {
        if let Some((guild, weecord_guild, channel)) = self.resolve_channel_and_guild(matches) {
            weecord_guild.autojoin_mut().push(channel.id());
            weecord_guild.write_config();
            tracing::info!(%weecord_guild.id, channel.id=%channel.id(), "Added channel to autojoin list");
            Weechat::print(&format!("Added channel {} to autojoin list", guild.name));
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
                crate::twilight_utils::search_cached_striped_guild_name(cache.as_ref(), &guild_name)
                    .await
            {
                tracing::trace!(%guild.name, "Matched guild");
                if let Some(channel) =
                    crate::twilight_utils::search_cached_stripped_guild_channel_name(
                        cache.as_ref(),
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
                    .subcommand(App::new("add").arg(Arg::with_name("name").required(true)))
                    .subcommand(
                        App::new("remove")
                            .arg(Arg::with_name("name").required(true))
                            .alias("rm"),
                    )
                    .subcommand(App::new("list")),
            )
            .subcommand(
                App::new("channel")
                    .subcommand(
                        App::new("autojoin")
                            .arg(Arg::with_name("guild_name").required(true))
                            .arg(Arg::with_name("name").required(true)),
                    )
                    .subcommand(
                        App::new("noautojoin")
                            .arg(Arg::with_name("guild_name").required(true))
                            .arg(Arg::with_name("name").required(true)),
                    ),
            );

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
            .add_argument("server add|remove|list <server-name>")
            .add_argument("channel autojoin|noautojoin <server-name> <channel-name>")
            .add_completion("server add|remove|list %(discord_guild)")
            .add_completion("channel autojoin|noautojoin %(discord_guild) %(discord_channel)"),
        DiscordCommand {
            session,
            connection,
            config,
        },
    )
}
