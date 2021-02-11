use std::process::{Command, Stdio};

fn main() -> std::io::Result<()> {
    let mut args = vec!["xtask".into()];
    args.extend(std::env::args().skip(1));

    Command::new("cargo")
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map(|_| ())
}
