use make_rs::*;

fn main() {
    let weechat_home = std::env::var("WEECHAT_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| dirs::home_dir().unwrap().join(".weechat/"));

    let test_dir = PathBuf::from(&env_or("WEECHAT_TEST_DIR", "./test_dir"));

    let test = {
        let test_dir = test_dir.clone();
        move || test(&test_dir)
    };

    let run_weechat = {
        let weechat_home = weechat_home.clone();
        move || run_weechat(&weechat_home)
    };

    let install_test = {
        let test_dir = test_dir.clone();
        move || install_test(&test_dir)
    };

    let install_release = {
        let weechat_home = weechat_home.clone();
        move || install_release(&weechat_home)
    };

    Maker::with()
        .default("test")
        .cmd("build_debug", debug)
        .cmd("install_test", install_test)
        .cmd("test", test)
        .cmd("build", release)
        .cmd("install", install_release)
        .cmd("run", run_weechat)
        .cmd("fmt", format)
        .make();
}

fn format() -> Result<()> {
    run("cargo", &["+nightly", "fmt"]).ignore()
}

fn run_weechat(weechat_home: &Path) -> Result<()> {
    install_release(weechat_home)?;
    run("weechat", &["-a"]).ignore()
}

fn release() -> Result<()> {
    run("cargo", &["build", "--release"]).ignore()
}

fn debug() -> Result<()> {
    run("cargo", &["build"]).ignore()
}

fn test(test_dir: &Path) -> Result<()> {
    install_test(test_dir)?;
    run("weechat", &["-d", &test_dir.to_string()]).ignore()
}

fn install_test(test_dir: &Path) -> Result<()> {
    debug()?;
    create_test_plugins_dir(test_dir)?;
    copy(glob("target/debug/libweecord.*"), test_dir.join("plugins/"))
}

fn install_release(dir: &Path) -> Result<()> {
    release()?;
    create_plugins_dir(dir)?;
    copy(glob("target/debug/libweecord.*"), dir.join("plugins/"))
}

fn create_test_plugins_dir(test_dir: &Path) -> Result<()> {
    create_dir(test_dir.join("plugins/"))
}

fn create_plugins_dir(dir: &Path) -> Result<()> {
    create_dir(dir.join("plugins/"))
}
