use std::io::BufRead;

use clap::Parser;
use url::Url;

use crate::connection::{ConnectionSpec, Secret};
use crate::store::settings::LogLevel;

const DEFAULT_HOST: &str = "127.0.0.1";
const DEFAULT_USER: &str = "root";

#[derive(Parser, Debug)]
#[command(
    name = "databee",
    version,
    about = "Minimalistic SQL client",
    after_help = "Examples:\n  databee mysql://wp@127.0.0.1:3306/wordpress --password-env DB_PASS\n  echo secret | databee -u wp -d wordpress --password-stdin\n  databee --socket ~/Library/.../mysqld.sock -u root -d local"
)]
struct Args {
    /// Connection URL: mysql://user[:password]@host[:port][/database][?socket=path]
    url: Option<String>,
    /// Server host (default 127.0.0.1)
    #[arg(short = 'H', long)]
    host: Option<String>,
    /// Server port (default from settings, usually 3306)
    #[arg(short = 'P', long)]
    port: Option<u16>,
    /// Unix socket path, e.g. for Local or MAMP
    #[arg(short = 'S', long)]
    socket: Option<String>,
    /// User name (default root)
    #[arg(short, long)]
    user: Option<String>,
    /// Database to open
    #[arg(short, long, visible_alias = "db")]
    database: Option<String>,
    /// Label shown in the UI
    #[arg(short, long)]
    name: Option<String>,
    /// Password (visible in the process list; prefer --password-stdin or --password-env)
    #[arg(short, long, group = "pw")]
    password: Option<String>,
    /// Read the password from the first line of stdin
    #[arg(long, group = "pw")]
    password_stdin: bool,
    /// Read the password from this environment variable
    #[arg(long, value_name = "VAR", group = "pw")]
    password_env: Option<String>,
    /// Store the password in the macOS Keychain after a successful connect
    #[arg(long)]
    save_password: bool,
    /// Log level for this run
    #[arg(long, value_enum)]
    log_level: Option<CliLogLevel>,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug)]
enum CliLogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl From<CliLogLevel> for LogLevel {
    fn from(l: CliLogLevel) -> Self {
        match l {
            CliLogLevel::Error => LogLevel::Error,
            CliLogLevel::Warn => LogLevel::Warn,
            CliLogLevel::Info => LogLevel::Info,
            CliLogLevel::Debug => LogLevel::Debug,
            CliLogLevel::Trace => LogLevel::Trace,
        }
    }
}

/// A connection request from the command line, including its password if one was given.
#[derive(Debug, Clone, PartialEq)]
pub struct Launch {
    pub spec: ConnectionSpec,
    pub password: Option<Secret>,
    pub save_password: bool,
}

#[derive(Debug, PartialEq)]
pub enum CliOutcome {
    Run {
        launch: Option<Launch>,
        log_level: Option<LogLevel>,
    },
    /// `--help` / `--version`: print and exit without starting the app.
    Exit { message: String },
}

/// Parses the full argv (including the program name).
/// `env` looks up environment variables; `stdin` is read only with `--password-stdin`.
pub fn parse<I, S>(
    args: I,
    default_port: u16,
    stdin: &mut dyn BufRead,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<CliOutcome, String>
where
    I: IntoIterator<Item = S>,
    S: Into<std::ffi::OsString> + Clone,
{
    let args = match Args::try_parse_from(args) {
        Ok(args) => args,
        Err(e) => {
            return match e.kind() {
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion => {
                    Ok(CliOutcome::Exit {
                        message: e.to_string(),
                    })
                }
                _ => Err(e.to_string()),
            };
        }
    };
    let log_level = args.log_level.map(Into::into);
    let launch = build_launch(args, default_port, stdin, env)?;
    Ok(CliOutcome::Run { launch, log_level })
}

fn build_launch(
    args: Args,
    default_port: u16,
    stdin: &mut dyn BufRead,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<Option<Launch>, String> {
    let from_url = match &args.url {
        Some(raw) => Some(parse_url(raw)?),
        None => None,
    };
    let wants_connection = from_url.is_some()
        || args.host.is_some()
        || args.socket.is_some()
        || args.user.is_some()
        || args.database.is_some();
    if !wants_connection {
        return Ok(None);
    }
    let (url_spec, url_password) = from_url.unwrap_or_default();

    let spec = ConnectionSpec {
        name: args.name,
        host: args
            .host
            .or(url_spec.host)
            .unwrap_or_else(|| DEFAULT_HOST.into()),
        port: args.port.or(url_spec.port).unwrap_or(default_port),
        socket: args.socket.or(url_spec.socket),
        user: args
            .user
            .or(url_spec.user)
            .unwrap_or_else(|| DEFAULT_USER.into()),
        database: args.database.or(url_spec.database),
    };

    let password = if let Some(p) = args.password {
        Some(Secret::new(p))
    } else if args.password_stdin {
        Some(read_password_line(stdin)?)
    } else if let Some(var) = args.password_env {
        Some(Secret::new(
            env(&var).ok_or_else(|| format!("Environment variable {var} is not set"))?,
        ))
    } else {
        url_password
    };

    Ok(Some(Launch {
        spec,
        password,
        save_password: args.save_password,
    }))
}

#[derive(Default)]
struct UrlParts {
    host: Option<String>,
    port: Option<u16>,
    socket: Option<String>,
    user: Option<String>,
    database: Option<String>,
}

fn parse_url(raw: &str) -> Result<(UrlParts, Option<Secret>), String> {
    let url = Url::parse(raw).map_err(|e| format!("Invalid connection URL: {e}"))?;
    if !matches!(url.scheme(), "mysql" | "mariadb") {
        return Err(format!(
            "Unsupported URL scheme '{}', expected mysql:// or mariadb://",
            url.scheme()
        ));
    }
    let decode = |s: &str| {
        percent_decode(s).map_err(|_| "Connection URL contains invalid percent-encoding".to_string())
    };
    let non_empty = |s: String| (!s.is_empty()).then_some(s);

    let user = non_empty(decode(url.username())?);
    let password = url.password().map(decode).transpose()?.map(Secret::new);
    let database = non_empty(decode(url.path().trim_start_matches('/'))?);
    let socket = url
        .query_pairs()
        .find(|(k, _)| k == "socket")
        .map(|(_, v)| v.into_owned());

    Ok((
        UrlParts {
            host: url.host_str().and_then(|h| non_empty(h.to_string())),
            port: url.port(),
            socket,
            user,
            database,
        },
        password,
    ))
}

fn percent_decode(s: &str) -> Result<String, std::str::Utf8Error> {
    percent_encoding::percent_decode_str(s)
        .decode_utf8()
        .map(|c| c.into_owned())
}

fn read_password_line(stdin: &mut dyn BufRead) -> Result<Secret, String> {
    let mut line = String::new();
    stdin
        .read_line(&mut line)
        .map_err(|e| format!("Cannot read password from stdin: {e}"))?;
    Ok(Secret::new(line.trim_end_matches(['\n', '\r'])))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(args: &[&str]) -> Result<CliOutcome, String> {
        run_with(args, "", &|_| None)
    }

    fn run_with(
        args: &[&str],
        stdin: &str,
        env: &dyn Fn(&str) -> Option<String>,
    ) -> Result<CliOutcome, String> {
        let argv = std::iter::once("databee").chain(args.iter().copied());
        parse(argv, 3306, &mut stdin.as_bytes(), env)
    }

    fn launch(args: &[&str]) -> Launch {
        match run(args).unwrap() {
            CliOutcome::Run {
                launch: Some(l), ..
            } => l,
            other => panic!("expected a launch, got {other:?}"),
        }
    }

    #[test]
    fn no_arguments_starts_without_connection() {
        assert_eq!(
            run(&[]).unwrap(),
            CliOutcome::Run {
                launch: None,
                log_level: None
            }
        );
    }

    #[test]
    fn url_provides_all_connection_parts() {
        let l = launch(&["mysql://wp:s%40cret@db.local:3307/wordpress"]);
        assert_eq!(l.spec.host, "db.local");
        assert_eq!(l.spec.port, 3307);
        assert_eq!(l.spec.user, "wp");
        assert_eq!(l.spec.database.as_deref(), Some("wordpress"));
        assert_eq!(l.password, Some(Secret::new("s@cret")));
    }

    #[test]
    fn flags_override_url_parts() {
        let l = launch(&["mysql://wp@db.local/wordpress", "-d", "shop", "-P", "33306"]);
        assert_eq!(l.spec.database.as_deref(), Some("shop"));
        assert_eq!(l.spec.port, 33306);
        assert_eq!(l.spec.host, "db.local");
    }

    #[test]
    fn missing_parts_use_defaults() {
        let l = launch(&["-d", "wordpress"]);
        assert_eq!(l.spec.host, "127.0.0.1");
        assert_eq!(l.spec.port, 3306);
        assert_eq!(l.spec.user, "root");
        assert_eq!(l.password, None);
    }

    #[test]
    fn socket_can_come_from_flag_or_url() {
        assert_eq!(
            launch(&["-S", "/tmp/mysql.sock"]).spec.socket.as_deref(),
            Some("/tmp/mysql.sock")
        );
        assert_eq!(
            launch(&["mysql://root@localhost/local?socket=/tmp/m.sock"])
                .spec
                .socket
                .as_deref(),
            Some("/tmp/m.sock")
        );
    }

    #[test]
    fn password_is_read_from_first_stdin_line() {
        let outcome = run_with(&["-u", "wp", "--password-stdin"], "pa ss\nignored\n", &|_| None);
        let CliOutcome::Run {
            launch: Some(l), ..
        } = outcome.unwrap()
        else {
            panic!()
        };
        assert_eq!(l.password, Some(Secret::new("pa ss")));
    }

    #[test]
    fn password_is_read_from_named_env_var() {
        let env = |k: &str| (k == "DB_PASS").then(|| "envpw".to_string());
        let CliOutcome::Run {
            launch: Some(l), ..
        } = run_with(&["-u", "wp", "--password-env", "DB_PASS"], "", &env).unwrap()
        else {
            panic!()
        };
        assert_eq!(l.password, Some(Secret::new("envpw")));
    }

    #[test]
    fn unset_password_env_var_is_an_error() {
        let err = run(&["-u", "wp", "--password-env", "NOPE"]).unwrap_err();
        assert!(err.contains("NOPE"));
    }

    #[test]
    fn explicit_password_flag_beats_url_password() {
        let l = launch(&["mysql://wp:urlpw@h/db", "-p", "flagpw"]);
        assert_eq!(l.password, Some(Secret::new("flagpw")));
    }

    #[test]
    fn password_sources_are_mutually_exclusive() {
        assert!(run(&["-u", "wp", "-p", "x", "--password-stdin"]).is_err());
    }

    #[test]
    fn unsupported_scheme_is_rejected() {
        let err = run(&["postgres://u@h/db"]).unwrap_err();
        assert!(err.contains("postgres"));
    }

    #[test]
    fn help_and_version_exit_without_launching() {
        assert!(matches!(run(&["--help"]).unwrap(), CliOutcome::Exit { .. }));
        assert!(matches!(run(&["--version"]).unwrap(), CliOutcome::Exit { .. }));
    }

    #[test]
    fn log_level_is_passed_through() {
        let CliOutcome::Run { log_level, .. } = run(&["--log-level", "debug"]).unwrap() else {
            panic!()
        };
        assert_eq!(log_level, Some(LogLevel::Debug));
    }
}
