# Weechat Discord

[![CI](https://github.com/terminal-discord/weechat-discord/workflows/CI/badge.svg)](https://github.com/terminal-discord/weechat-discord/actions)


A plugin that adds Discord to [Weechat](https://weechat.org/)

---

### Warning

***Usage of self-tokens is a violation of Discord's TOS***

This client makes use of the Discord "user api" and is could potentially viewed as a "self-bot".

This client does not abuse the api, however it is still a violation of the TOS and makes use of undocumented "client only" 
apis.

Use at your own risk: using this program could get your account or ip disabled, banned, etc.

---

### Table of Contents

* [Installation](#installation)
* [Setup](#setup)
* [Configuration](#configuration)
  * [Typing indicator](#typing-indicator)
* [Usage](#usage)
  * [Editing](#editing)
* [MacOS](#macos)


### Installation

Binaries are automatically compiled for macOS and linux and archived on [Github Actions](https://github.com/terminal-discord/weechat-discord/actions)

On macOS you will need to [adjust the Weechat plugin extensions](#macos)

### Building

In order to build weechat-discord yourself you will need:

* A recent version of [Rust](https://www.rust-lang.org/)
* Weechat developer libraries (optional)
* [libclang](https://rust-lang.github.io/rust-bindgen/requirements.html)

Compiling with Cargo with result in a shared object file `target/release/libweecord.so` (or `.dylib` on macos), which
then needs to be installed to the `plugins/` dir of your weechat home.

This can be done automatically with the project build tool.

```
cargo run -- install
```

Other commands include:

* `cargo run -- test` - Builds and installs in a test directory
* `cargo run -- run` - Builds and installs globally for release
* `cargo run -- fmt` - Builds and formats the repo
* `cargo run` - Is the same as `cargo run -- test`

The global install dir defaults to ~/.weechat and can be changed by setting `WEECHAT_HOME` and the test dir defaults to
`./test_dir/` and can be changed by setting `WEECHAT_TEST_DIR`

### Setup

You must first obtain a login token.

A Python script [`find_token.py`](find_token.py) is included which will attempt to find the tokens used by installed
Discord clients (both the webapp and desktop app should work).

The script will use ripgrep if installed to search faster.

If the script fails, you can get the tokens manually.

Open Devtools (ctrl+shift+i or ctrl+opt+i) and navigate to Application tab > Local Storage on left > discordapp.com > "token".
Discord deletes the token once the page has loaded, so you will need to refresh the page and to grab it.

Once you have your token you can run

```
/discord token 123456789ABCDEF
```

Although this saves your token insecurely in `$WEECHAT_HOME/weecord.conf`, so it is recommended you use 
[secure data](https://weechat.org/blog/post/2013/08/04/Secured-data).
Then, if you saved your token as `discord_token` then you would run

```
/discord token ${sec.data.discord_token}
```


### Configuration

#### Typing indicator

The bar item `discord_typing` displays the typing status of the current buffer and can be appended to
`weechat.bar.status.items`.


### Usage

#### Editing

Messages can be edited and deleted using ed style substitutions.

To edit the previous message:
```
s/foo/bar/
```

To delete the previous message:
```
s///
```

To select an older message, an offset can be included, for example, to delete the 3rd most recent message (sent by you):
```
3s///
```

### MacOS
Weechat does not search for mac dynamic libraries (.dylib) by default, this can be fixed by adding dylibs to the plugin search path,

```
/set weechat.plugin.extension ".so,.dll,.dylib"
```
