use std::{path::PathBuf, process::Command};
pub fn command(name: &str) -> Command {
    bundled(name).map_or_else(|| Command::new(name), Command::new)
}
pub fn bundled(name: &str) -> Option<PathBuf> {
    let extension = if cfg!(windows) { ".exe" } else { "" };
    let filename = format!("{name}{extension}");
    let executable = std::env::current_exe().ok()?;
    let directory = executable.parent()?;
    [
        directory.join("resources/bin").join(&filename),
        directory.join("bin").join(&filename),
        directory.join(&filename),
    ]
    .into_iter()
    .find(|path| path.is_file())
}
