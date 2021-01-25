use make_rs::*;

fn main() {
    let weechat_home = std::env::var("WEECHAT_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| dirs::home_dir().unwrap().join(".weechat/"));

    let test_dir = PathBuf::from(&env_or("WEECHAT_TEST_DIR", "./test_dir"));

    let test = || test(&test_dir);
    let run_weechat = || run_weechat(&weechat_home);
    let install_test = || install_test(&test_dir);
    let install_release = || install_release(&weechat_home);

    Maker::with()
        .default("test")
        .cmd("build_debug", debug)
        .cmd("install_test", install_test)
        .cmd("test", test)
        .cmd("build", release)
        .cmd("tests", tests)
        .cmd("check", check)
        .cmd("clippy", clippy)
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
    let mut args = vec!["build".to_string(), "--release".to_string()];
    if let Ok(features) = std::env::var("WEECORD_FEATURES") {
        args.push("--features".to_string());
        args.push(features);
    }
    run("cargo", &args).abort_on_failure()
}

fn debug() -> Result<()> {
    let mut args = vec!["build".to_string()];
    if let Ok(features) = std::env::var("WEECORD_FEATURES") {
        args.push("--features".to_string());
        args.push(features);
    }
    run("cargo", &args).abort_on_failure()
}

fn check() -> Result<()> {
    let mut args = vec!["check".to_string()];
    if let Ok(features) = std::env::var("WEECORD_FEATURES") {
        args.push("--features".to_string());
        args.push(features);
    }
    run("cargo", &args).abort_on_failure()
}

fn tests() -> Result<()> {
    let mut args = vec!["test".to_string()];
    if let Ok(features) = std::env::var("WEECORD_FEATURES") {
        args.push("--features".to_string());
        args.push(features);
    }
    run("cargo", &args).abort_on_failure()
}

fn clippy() -> Result<()> {
    let mut args = vec!["clippy".to_string()];
    if let Ok(features) = std::env::var("WEECORD_FEATURES") {
        args.push("--features".to_string());
        args.push(features);
    }
    run("cargo", &args).abort_on_failure()
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

trait ResultHelper2 {
    fn abort_on_failure(self) -> anyhow::Result<()>;
}

impl<E> ResultHelper2 for std::result::Result<std::process::ExitStatus, E> {
    fn abort_on_failure(self) -> anyhow::Result<()> {
        if let Ok(e) = self {
            if !e.success() {
                if let Some(code) = e.code() {
                    std::process::exit(code);
                } else {
                    std::process::exit(1);
                };
            }
        }
        Ok(())
    }
}
