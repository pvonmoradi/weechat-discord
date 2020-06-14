use crate::{
    config::Config, discord::discord_connection::DiscordConnection, guild_buffer::DiscordGuild,
    twilight_utils::GuildChannelExt, utils, DiscordSession,
};
use clap::{App, AppSettings, Arg, ArgMatches};
use twilight::model::id::GuildId;
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
        let cache = match &*self.connection.borrow() {
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
                for guild_id in cache
                    .guild_ids()
                    .await
                    .expect("InMemoryCache cannot fail")
                    .expect("guild_ids never fails")
                {
                    if let Some(guild) = cache
                        .guild(guild_id)
                        .await
                        .expect("InMemoryCache cannot fail")
                    {
                        if utils::clean_name(&guild_name) == utils::clean_name(&guild.name) {
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
                                Weechat::print(&format!(
                                    "\"{}\" has already been added",
                                    guild.name
                                ));
                            }
                            return;
                        }
                    } else {
                        tracing::warn!("{:?} not found in cache", guild_id);
                    }
                }
                tracing::info!("Could not find guild: \"{}\"", guild_name);
                Weechat::print(&format!("Could not find guild: {}", guild_name));
            });
        }
    }

    fn remove_guild(&self, matches: &ArgMatches) {
        let cache = match &*self.connection.borrow() {
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
                let guilds = session
                    .guilds
                    .borrow()
                    .keys()
                    .copied()
                    .collect::<Vec<GuildId>>();
                for guild_id in guilds {
                    if let Some(guild) = cache
                        .guild(guild_id)
                        .await
                        .expect("InMemoryCache cannot fail")
                    {
                        if utils::clean_name(&guild_name) == utils::clean_name(&guild.name) {
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
                            return;
                        }
                    } else {
                        tracing::warn!("{:?} not found in cache", guild_id);
                    }
                }
                tracing::info!("Could not find guild: \"{}\"", guild_name);
                Weechat::print(&format!("Could not find guild: {}", guild_name));
            });
        }
    }

    fn list_guilds(&self) {
        Weechat::print("discord: Servers:");

        if let Some(connection) = &*self.connection.borrow() {
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
}

impl weechat::hooks::CommandCallback for DiscordCommand {
    fn callback(&mut self, _: &Weechat, _: &Buffer, arguments: ArgsWeechat) {
        let args = arguments.collect::<Vec<_>>();

        let app = App::new("/discord").subcommand(
            clap::App::new("server")
                .global_setting(AppSettings::DisableVersion)
                .global_setting(AppSettings::VersionlessSubcommands)
                .setting(AppSettings::SubcommandRequiredElseHelp)
                .subcommand(App::new("add").arg(Arg::new("name").required(true)))
                .subcommand(
                    App::new("remove")
                        .arg(Arg::new("name").required(true))
                        .alias("rm"),
                )
                .subcommand(App::new("list")),
        );

        let matches = match app.try_get_matches_from(args) {
            Ok(m) => m,
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
            .add_completion("server add|remove|list %(discord_guild)"),
        DiscordCommand {
            session,
            connection,
            config,
        },
    )
}
