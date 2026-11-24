use crate::tlc_env::Jit;
use anyhow::{bail, Result};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::OnceLock;

/// How `verify` starts TLC, and the JIT settings its JVM was seen to take.
pub(crate) struct Tlc {
    launch: Launch,
    /// Whether the JVM starts with `Jit::C1`'s flags; asked on the first run that wants them.
    c1: OnceLock<bool>,
}

/// A `tlc` / `tlc2` launcher on PATH, or `java -cp <jar> tlc2.TLC` when `--tlc-jar` or
/// `RDRA_TLC_JAR` names `tla2tools.jar`.
enum Launch {
    Launcher(PathBuf),
    Jar(PathBuf),
}

/// The env variable that names `tla2tools.jar` when `--tlc-jar` is not given.
pub(crate) const TLC_JAR_ENV: &str = "RDRA_TLC_JAR";

/// What a JVM prints when it does not start with the options it was given.
const JVM_REFUSED: [&str; 4] = [
    "Unrecognized VM option",
    "Improperly specified VM option",
    "Could not create the Java Virtual Machine",
    "Error occurred during initialization of VM",
];

impl Tlc {
    /// `--tlc-jar` first, then `RDRA_TLC_JAR`, then `tlc` and `tlc2` on PATH.
    pub(super) fn find(jar: Option<PathBuf>) -> Result<Tlc> {
        let jar = jar.or_else(|| {
            std::env::var_os(TLC_JAR_ENV)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        });
        let launch = match jar {
            Some(jar) if !jar.is_file() => bail!(
                "tla2tools.jar not found at {} (from --tlc-jar or {TLC_JAR_ENV})",
                jar.display()
            ),
            Some(jar) => Launch::Jar(jar),
            None => match ["tlc", "tlc2"].into_iter().find_map(find_on_path) {
                Some(path) => Launch::Launcher(path),
                None => bail!(
                    "TLC not found on PATH (tried `tlc`, `tlc2`)\n\
                     Get it one of these ways:\n  \
                     - Nix: `nix shell nixpkgs#tlaplus` (add `--extra-experimental-features 'nix-command flakes'` \
                     if flakes are off), then run verify in that shell\n  \
                     - Download tla2tools.jar from https://github.com/tlaplus/tlaplus/releases and pass \
                     `--tlc-jar <path>` or set {TLC_JAR_ENV}=<path> (needs `java` on PATH)\n\
                     Or use `rdra-ish export --kind tla` to generate the spec without running TLC."
                ),
            },
        };
        Ok(Tlc {
            launch,
            c1: OnceLock::new(),
        })
    }

    pub(super) fn command(&self) -> Command {
        match &self.launch {
            Launch::Launcher(path) => Command::new(path),
            Launch::Jar(jar) => {
                let mut command = Command::new("java");
                command.arg("-cp").arg(jar).arg("tlc2.TLC");
                command
            }
        }
    }

    /// The batch driver of this TLC (#310): `tlc-batch` in the directory of the `tlc`
    /// launcher, as the flake builds the two from one jar and one JDK. `None` for a jar
    /// the user names, for a `tlc` with no such neighbour, and with `RDRA_TLC_BATCH=0`.
    pub(super) fn batch_launcher(&self) -> Option<PathBuf> {
        let Launch::Launcher(tlc) = &self.launch else {
            return None;
        };
        if std::env::var(crate::tlc_batch::BATCH_ENV).as_deref() == Ok("0") {
            return None;
        }
        let batch = tlc.parent()?.join(crate::tlc_batch::LAUNCHER);
        batch.is_file().then_some(batch)
    }

    pub(super) fn display(&self) -> String {
        match &self.launch {
            Launch::Launcher(path) => path.display().to_string(),
            Launch::Jar(jar) => format!("java -cp {} tlc2.TLC", jar.display()),
        }
    }

    /// `wanted` when this TLC's JVM starts with its flags, else `Jit::Tiered`. The JVM is
    /// whatever `java` the launcher or PATH gives, which `verify` cannot see, and an unknown
    /// `-XX` flag stops it (JDK 8 stops on one even after `-XX:+IgnoreUnrecognizedVMOptions`),
    /// so the first run that wants C1 first starts TLC once with the flags and `-help`
    /// (0.05 s), and every later run reads the answer.
    pub(super) fn jit(&self, wanted: Jit) -> Jit {
        match wanted {
            Jit::Tiered => Jit::Tiered,
            Jit::C1 if *self.c1.get_or_init(|| self.starts_with(Jit::C1.flags())) => Jit::C1,
            Jit::C1 => Jit::Tiered,
        }
    }

    /// Whether TLC starts with `flags` as `JAVA_TOOL_OPTIONS`. TLC's `-help` prints the usage
    /// and exits 1, so only the JVM's own refusal says no.
    fn starts_with(&self, flags: &str) -> bool {
        let output = self
            .command()
            .arg("-help")
            .env("JAVA_TOOL_OPTIONS", flags)
            .stdin(Stdio::null())
            .output();
        let Ok(output) = output else {
            return false;
        };
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        !JVM_REFUSED.iter().any(|refused| text.contains(refused))
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
