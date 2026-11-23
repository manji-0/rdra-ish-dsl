use anyhow::{bail, Result};
use std::path::PathBuf;
use std::process::Command;

/// How `verify` starts TLC: a `tlc` / `tlc2` launcher on PATH, or `java -cp <jar> tlc2.TLC`
/// when `--tlc-jar` or `RDRA_TLC_JAR` names `tla2tools.jar`.
pub(crate) enum Tlc {
    Launcher(PathBuf),
    Jar(PathBuf),
}

/// The env variable that names `tla2tools.jar` when `--tlc-jar` is not given.
pub(crate) const TLC_JAR_ENV: &str = "RDRA_TLC_JAR";

impl Tlc {
    /// `--tlc-jar` first, then `RDRA_TLC_JAR`, then `tlc` and `tlc2` on PATH.
    pub(super) fn find(jar: Option<PathBuf>) -> Result<Tlc> {
        let jar = jar.or_else(|| {
            std::env::var_os(TLC_JAR_ENV)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        });
        if let Some(jar) = jar {
            if !jar.is_file() {
                bail!(
                    "tla2tools.jar not found at {} (from --tlc-jar or {TLC_JAR_ENV})",
                    jar.display()
                );
            }
            return Ok(Tlc::Jar(jar));
        }
        for candidate in ["tlc", "tlc2"] {
            if let Some(path) = find_on_path(candidate) {
                return Ok(Tlc::Launcher(path));
            }
        }
        bail!(
            "TLC not found on PATH (tried `tlc`, `tlc2`)\n\
             Get it one of these ways:\n  \
             - Nix: `nix shell nixpkgs#tlaplus` (add `--extra-experimental-features 'nix-command flakes'` \
             if flakes are off), then run verify in that shell\n  \
             - Download tla2tools.jar from https://github.com/tlaplus/tlaplus/releases and pass \
             `--tlc-jar <path>` or set {TLC_JAR_ENV}=<path> (needs `java` on PATH)\n\
             Or use `rdra-ish export --kind tla` to generate the spec without running TLC."
        );
    }

    pub(super) fn command(&self) -> Command {
        match self {
            Tlc::Launcher(path) => Command::new(path),
            Tlc::Jar(jar) => {
                let mut command = Command::new("java");
                command.arg("-cp").arg(jar).arg("tlc2.TLC");
                command
            }
        }
    }

    pub(super) fn display(&self) -> String {
        match self {
            Tlc::Launcher(path) => path.display().to_string(),
            Tlc::Jar(jar) => format!("java -cp {} tlc2.TLC", jar.display()),
        }
    }
}

/// The first `name` on PATH that is a file, also trying the platform's executable suffix.
pub(crate) fn find_on_path(name: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    let exe = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    std::env::split_paths(&paths).find_map(|dir| {
        [name, exe.as_str()]
            .into_iter()
            .map(|file| dir.join(file))
            .find(|path| path.is_file())
    })
}
