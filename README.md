# Weechat Discord


## Warning

The developer of [cordless](https://github.com/Bios-Marcel/cordless) (another 3rd party client) has had his [account banned for using a 3rd party client](https://github.com/Bios-Marcel/cordless#i-am-closing-down-the-cordless-project). 

***It is very possible Discord is now actively enforcing TOS violations, I cannot recommending using this project with an account you are not ok with loosing***

---

***Usage of self-tokens is a violation of Discord's TOS***

This client makes use of the "user api" and is essentially a self-bot.
This client does not abuse the api however it is still a violation of the TOS.

Use at your own risk, using this program could get your account or ip disabled, banned, etc.

---

![CI](https://github.com/terminal-discord/weechat-discord/workflows/CI/badge.svg)
[![Discord](https://img.shields.io/discord/715036059712356372?label=discord&logo=discord&logoColor=white)](https://discord.gg/BcPku6R)

A plugin that adds Discord to [Weechat](https://weechat.org/)

(Beta)

This branch (`master`) is now in maintenance mode as the plugin is being rewritten in the `mk3` branch.

---

### Installation

Binaries are automatically compiled for macOS and linux on [Github Actions](https://terminal-discord.vercel.app/api/latest-build?repo=weechat-discord&workflow=1329556&branch=master&redirect)

#### Building

Dependencies:

* Weechat developer libraries. Usually called `weechat-dev`, or sometimes just `weechat` includes them.
* [Rust](https://www.rust-lang.org). Ensure you have the latest version.
* [libclang](https://rust-lang.github.io/rust-bindgen/requirements.html)

Then just run `make install`

    cd weechat-discord # or wherever you cloned it
    make install

This will produce a shared object called `target/release/libweecord.so` (or `.dylib` on macos). Place it in your weechat plugins directory, which is probably located at `~/.weechat/plugins` (may need to be created)

The Makefile has several other development commands:

    make # (same as make all) just runs that `cargo build --release` command, produces weecord.so
    make install # builds and copies the .so to ~/.weechat/plugins, creating the dir if required
    make test # install to ./test_dir/ and opens weechat with that dir
    make run # installs and runs `weechat -a` (-a means "don't autoconnect to servers")

Quitting weechat before installing is recommended

### Set up

[You will need to obtain a login token](https://github.com/discordapp/discord-api-docs/issues/69#issuecomment-223886862).
You can either use a python script to find the tokens, or try and grab them manually.

#### Python Script

`find_token.py` is a simple python3 script to search the computer for localstorage databases. It will present a list of all found databases.

If ripgrep is installed it will use that, if not, it will use `find`.


#### Manually

In the devtools menu of the website and desktop app (ctrl+shift+i or ctrl+opt+i) Application tab > Local Storage on left, discordapp.com, token entry.

When this was written, discord deletes its token from the visible table, so you may need to refresh the page (ctrl/cmd+r) and grab the token as it is refreshing.


### Usage

First, you either need to load the plugin, or have it set to autoload.

Then, set your token:

    /discord token 123456789ABCDEF
   
This saves the discord token in `<weechatdir>/plugins.conf`, **so make sure not to commit this file or share it with anyone.**

You can also secure your token with [secure data](https://weechat.org/blog/post/2013/08/04/Secured-data).
If you saved your token as `discord_token` then you would run

    /discord token ${sec.data.discord_token}

Then, connect:

    /discord connect

If you want to always connect on load, you can enable autostart with:

    /discord autostart

Note you may also have to adjust a few settings for best use:

    weechat.bar.status.items -> replace buffer_name with buffer_short_name
    # additionally, buffer_guild_name, buffer_channel_name, and buffer_discord_full_name bar
    # items can be used
    plugins.var.python.go.short_name -> on (if you use go.py)

If you want a more irc-style interface, you can enable irc-mode:

    /discord irc-mode

In irc-mode, weecord will not automatically "join" every Discord channel.  You must join a channel using the
`/discord join <guild-name> [<channel-name>]` command.

Watched channels:  
You can use `/discord watch <guild-name> [<channel-name>]` to start watching a channel or entire guild.
This means that if a message is received in a watched channel, that channel will be joined and added to the nicklist.

Autojoin channels:  
You can use `/discord autojoin <guild-name> [<channel-name>]` to start watching a channel or entire guild.
Any channel or guild marked as autojoin will be automatically joined when weecord connects.

A typing indicator can be added with the `discord_typing` bar item by appending `,discord_typing` to `weechat.bar.status.items`.

Messages can be edited and deleted using ed style substitutions.

To edit:

    s/foo/bar/

To delete:
    
    s///

An optional message id can also be passed to target the nth most recent message:

    3s///

---

## MacOS

Weechat does not search for mac dynamic libraries (.dylib) by default, this can be fixed by adding dylibs to the plugin search path,

```
/set weechat.plugin.extension ".so,.dll,.dylib"
```
