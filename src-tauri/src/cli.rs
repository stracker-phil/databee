use std::io::BufRead;

use clap::{Parser, Subcommand};
use url::{Host, Url};

use crate::address::{Address, SCHEME};
use crate::connection::{ConnectionSpec, Secret};
use crate::ipc::IpcMessage;
use crate::store::settings::LogLevel;

const DEFAULT_HOST: &str = "127.0.0.1";
const DEFAULT_USER: &str = "root";

#[derive(Parser, Debug)]
#[command(
    name = "databee",
    version,
    about = "Minimalistic SQL client",
    args_conflicts_with_subcommands = true,
    after_help = "Examples:\n  databee mysql://wp@127.0.0.1:3306/site --password-env DB_PASS\n  echo secret | databee -u wp -d site --password-stdin\n  databee --socket ~/Library/.../mysqld.sock -u root -d local\n  databee open databee://shop/site/wp_posts/42\n  databee open order 4711 --connection shop\n  databee command view.sidebar\n  claude mcp add databee -- databee mcp"
)]
struct Args {
    #[command(subcommand)]
    command: Option<CliCommand>,
    /// Connection URL: mysql://user[:password]@host[:port][/database][?socket=path].
    /// A password in the URL stays visible in the process list while this process runs the
    /// app; prefer --password-stdin or --password-env
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
    #[arg(short, long, group = "pw", value_parser = parse_secret)]
    password: Option<Secret>,
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
    log_level: Option<LogLevel>,
}

#[derive(Subcommand, Debug)]
enum CliCommand {
    /// Open an address, or `<entity> <id>`, in the running DataBee
    Open {
        /// A databee:// address, or an entity name such as `order`
        target: String,
        /// The entity's key, when TARGET is an entity name
        id: Option<String>,
        /// Connection slug for `<entity> <id>` (default: the active connection)
        #[arg(short, long)]
        connection: Option<String>,
        /// Open without bringing DataBee to the front
        #[arg(long)]
        background: bool,
    },
    /// Serve a read-only MCP server on stdio (for Claude Code: `claude mcp add databee -- databee mcp`)
    Mcp,
    /// Run a command of the running DataBee by its id, e.g. `view.sidebar`
    Command {
        /// The command id, as listed in the keybindings file
        id: String,
    },
}

/// A connection request from the command line, including its password if one was given.
#[derive(Debug, Clone)]
#[cfg_attr(test, derive(PartialEq))]
pub struct Launch {
    pub spec: ConnectionSpec,
    pub password: Option<Secret>,
    pub save_password: bool,
}

#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub enum CliOutcome {
    Run {
        launch: Option<Launch>,
        log_level: Option<LogLevel>,
    },
    /// `--help` / `--version`: print and exit without starting the app.
    Exit { message: String },
    /// A subcommand for the running app (`open`, `command`).
    Remote { message: IpcMessage },
    /// `databee mcp`: serve MCP on stdio instead of starting the app.
    Mcp,
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
    if let Some(command) = args.command {
        return subcommand_outcome(command);
    }
    let log_level = args.log_level;
    let launch = build_launch(args, default_port, stdin, env)?;
    Ok(CliOutcome::Run { launch, log_level })
}

fn subcommand_outcome(command: CliCommand) -> Result<CliOutcome, String> {
    let message = match command {
        CliCommand::Open {
            target,
            id,
            connection,
            background,
        } => {
            let address = match id {
                Some(id) => entity_target(&target, &id)?,
                None => address_target(target, connection.is_some())?,
            };
            IpcMessage::Open {
                address,
                connection,
                focus: !background,
            }
        }
        CliCommand::Command { id } => IpcMessage::Command { id },
        CliCommand::Mcp => return Ok(CliOutcome::Mcp),
    };
    Ok(CliOutcome::Remote { message })
}

fn address_target(target: String, has_connection: bool) -> Result<String, String> {
    if !target.starts_with(SCHEME) {
        return Err(format!(
            "Expected a {SCHEME} address or `<entity> <id>`, got `{target}`"
        ));
    }
    if has_connection {
        return Err(
            "--connection applies to `<entity> <id>`; an address names its own connection".into(),
        );
    }
    Address::parse(&target).map_err(|e| e.to_string())?;
    Ok(target)
}

/// `<entity> <id>` as the shorthand the app resolves.
fn entity_target(entity: &str, id: &str) -> Result<String, String> {
    let id = id.trim();
    if entity.starts_with(SCHEME) {
        return Err(format!("An address takes no id: {entity} {id}"));
    }
    if entity.contains(char::is_whitespace) || id.is_empty() {
        return Err(format!("Expected `<entity> <id>`, got `{entity} {id}`"));
    }
    Ok(format!("{entity} {id}"))
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
        || args.port.is_some()
        || args.socket.is_some()
        || args.user.is_some()
        || args.database.is_some()
        || args.name.is_some()
        || args.password.is_some()
        || args.password_stdin
        || args.password_env.is_some();
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

    let password = if args.password.is_some() {
        args.password
    } else if args.password_stdin {
        Some(read_password_line(stdin)?)
    } else if let Some(var) = args.password_env {
        Some(Secret::new(env(&var).ok_or_else(|| {
            format!("Environment variable {var} is not set")
        })?))
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
        percent_decode(s)
            .map_err(|_| "Connection URL contains invalid percent-encoding".to_string())
    };
    let non_empty = |s: String| (!s.is_empty()).then_some(s);

    let user = non_empty(decode(url.username())?);
    let password = url.password().map(decode).transpose()?.map(Secret::new);
    let segments: Vec<&str> = url
        .path_segments()
        .map(|s| s.filter(|s| !s.is_empty()).collect())
        .unwrap_or_default();
    let database = match segments.as_slice() {
        [] => None,
        [db] => non_empty(decode(db)?),
        _ => return Err("Connection URL path must name a single database".into()),
    };
    let socket = url
        .query_pairs()
        .find(|(k, _)| k == "socket")
        .and_then(|(_, v)| non_empty(v.into_owned()));
    let host = url.host().and_then(|h| match h {
        // `Host::to_string` keeps the brackets, which sqlx can't resolve.
        Host::Ipv6(addr) => Some(addr.to_string()),
        other => non_empty(other.to_string()),
    });

    Ok((
        UrlParts {
            host,
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

fn parse_secret(raw: &str) -> Result<Secret, std::convert::Infallible> {
    Ok(Secret::new(raw))
}

fn read_password_line(stdin: &mut dyn BufRead) -> Result<Secret, String> {
    let mut line = String::new();
    let read = stdin
        .read_line(&mut line)
        .map_err(|e| format!("Cannot read password from stdin: {e}"))?;
    if read == 0 {
        return Err("--password-stdin was given but stdin is empty".into());
    }
    let line = line.strip_suffix('\n').unwrap_or(&line);
    Ok(Secret::new(line.strip_suffix('\r').unwrap_or(line)))
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
        let l = launch(&["mysql://wp:s%40cret@db.local:3307/site"]);
        assert_eq!(l.spec.host, "db.local");
        assert_eq!(l.spec.port, 3307);
        assert_eq!(l.spec.user, "wp");
        assert_eq!(l.spec.database.as_deref(), Some("site"));
        assert_eq!(l.password, Some(Secret::new("s@cret")));
    }

    #[test]
    fn flags_override_url_parts() {
        let l = launch(&["mysql://wp@db.local/site", "-d", "shop", "-P", "33306"]);
        assert_eq!(l.spec.database.as_deref(), Some("shop"));
        assert_eq!(l.spec.port, 33306);
        assert_eq!(l.spec.host, "db.local");
    }

    #[test]
    fn missing_parts_use_defaults() {
        let l = launch(&["-d", "site"]);
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
        let outcome = run_with(
            &["-u", "wp", "--password-stdin"],
            "pa ss\nignored\n",
            &|_| None,
        );
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
        let pairs: [&[&str]; 3] = [
            &["-p", "x", "--password-stdin"],
            &["-p", "x", "--password-env", "V"],
            &["--password-stdin", "--password-env", "V"],
        ];
        for args in pairs {
            assert!(run(args).is_err(), "{args:?}");
        }
    }

    #[test]
    fn ipv6_url_host_is_kept_without_brackets() {
        let l = launch(&["mysql://u@[::1]:3307/db"]);
        assert_eq!(l.spec.host, "::1");
        assert_eq!(l.spec.port, 3307);
    }

    #[test]
    fn mariadb_scheme_is_accepted() {
        let l = launch(&["mariadb://u@h/db"]);
        assert_eq!(l.spec.host, "h");
        assert_eq!(l.spec.database.as_deref(), Some("db"));
    }

    #[test]
    fn invalid_percent_encoding_in_url_is_an_error() {
        let err = run(&["mysql://u:%ff@h/db"]).unwrap_err();
        assert!(err.contains("percent-encoding"), "{err}");
    }

    #[test]
    fn empty_url_user_falls_back_to_root() {
        assert_eq!(launch(&["mysql://@h/db"]).spec.user, "root");
    }

    #[test]
    fn password_stdin_with_empty_stdin_is_an_error() {
        let err = run_with(&["--password-stdin"], "", &|_| None).unwrap_err();
        assert!(err.contains("stdin"), "{err}");
    }

    #[test]
    fn crlf_is_stripped_from_the_stdin_password() {
        let CliOutcome::Run {
            launch: Some(l), ..
        } = run_with(&["--password-stdin"], "pw\r\n", &|_| None).unwrap()
        else {
            panic!()
        };
        assert_eq!(l.password, Some(Secret::new("pw")));
    }

    #[test]
    fn name_flag_is_carried_into_the_spec() {
        assert_eq!(
            launch(&["-n", "Shop", "-d", "db"]).spec.name.as_deref(),
            Some("Shop")
        );
    }

    #[test]
    fn save_password_flag_is_carried() {
        assert!(launch(&["-d", "db", "--save-password"]).save_password);
        assert!(!launch(&["-d", "db"]).save_password);
    }

    #[test]
    fn port_flag_alone_requests_a_connection() {
        let l = launch(&["-P", "3307"]);
        assert_eq!(l.spec.port, 3307);
        assert_eq!(l.spec.host, "127.0.0.1");
    }

    #[test]
    fn empty_socket_query_is_no_socket() {
        assert_eq!(launch(&["mysql://u@h/db?socket="]).spec.socket, None);
    }

    #[test]
    fn host_flag_overrides_url_host() {
        assert_eq!(
            launch(&["mysql://u@h/db", "-H", "other"]).spec.host,
            "other"
        );
    }

    #[test]
    fn password_env_beats_url_password() {
        let env = |k: &str| (k == "V").then(|| "envpw".to_string());
        let CliOutcome::Run {
            launch: Some(l), ..
        } = run_with(&["mysql://u:urlpw@h/db", "--password-env", "V"], "", &env).unwrap()
        else {
            panic!()
        };
        assert_eq!(l.password, Some(Secret::new("envpw")));
    }

    #[test]
    fn url_path_with_several_segments_is_rejected() {
        assert!(run(&["mysql://h/db/extra"]).is_err());
    }

    #[test]
    fn unsupported_scheme_is_rejected() {
        let err = run(&["postgres://u@h/db"]).unwrap_err();
        assert!(err.contains("postgres"));
    }

    #[test]
    fn help_and_version_exit_without_launching() {
        assert!(matches!(run(&["--help"]).unwrap(), CliOutcome::Exit { .. }));
        assert!(matches!(
            run(&["--version"]).unwrap(),
            CliOutcome::Exit { .. }
        ));
    }

    fn remote(args: &[&str]) -> IpcMessage {
        match run(args).unwrap() {
            CliOutcome::Remote { message } => message,
            other => panic!("expected a remote message, got {other:?}"),
        }
    }

    fn open(address: &str, connection: Option<&str>, focus: bool) -> IpcMessage {
        IpcMessage::Open {
            address: address.into(),
            connection: connection.map(Into::into),
            focus,
        }
    }

    #[test]
    fn positional_connection_url_still_starts_the_app() {
        let outcome = run(&["mysql://wp@h/db"]).unwrap();
        assert!(matches!(
            outcome,
            CliOutcome::Run {
                launch: Some(_),
                ..
            }
        ));
    }

    #[test]
    fn open_with_an_address_sends_it_and_focuses() {
        let address = "databee://shop/site/wp_posts/42";
        assert_eq!(remote(&["open", address]), open(address, None, true));
    }

    #[test]
    fn open_with_entity_and_id_joins_them_into_one_address() {
        assert_eq!(
            remote(&["open", "order", "4711"]),
            open("order 4711", None, true)
        );
    }

    #[test]
    fn connection_flag_sets_the_connection_in_both_spellings() {
        for flag in ["-c", "--connection"] {
            assert_eq!(
                remote(&["open", "order", "4711", flag, "shop"]),
                open("order 4711", Some("shop"), true),
                "{flag}"
            );
        }
    }

    #[test]
    fn background_flag_opens_without_focus() {
        assert_eq!(
            remote(&["open", "order", "4711", "--background"]),
            open("order 4711", None, false)
        );
    }

    #[test]
    fn command_subcommand_sends_the_command_id() {
        assert_eq!(
            remote(&["command", "view.sidebar"]),
            IpcMessage::Command {
                id: "view.sidebar".into()
            }
        );
    }

    #[test]
    fn mcp_subcommand_serves_instead_of_starting_the_app() {
        assert_eq!(run(&["mcp"]).unwrap(), CliOutcome::Mcp);
    }

    #[test]
    fn connection_flags_cannot_be_mixed_with_mcp() {
        assert!(run(&["-d", "db", "mcp"]).is_err());
    }

    #[test]
    fn invalid_open_targets_are_rejected() {
        for args in [
            &["open", "databee://x"][..],
            &["open", "order"],
            &["open", "databee://shop/db/wp_posts/1", "42"],
            &[
                "open",
                "databee://shop/db/wp_posts/1",
                "--connection",
                "shop",
            ],
            &["open", "order", " "],
            &["open", "two words", "1"],
        ] {
            assert!(run(args).is_err(), "{args:?} should be rejected");
        }
    }

    #[test]
    fn connection_flags_cannot_be_mixed_with_a_subcommand() {
        assert!(run(&["-d", "db", "open", "databee://shop/db/wp_posts/1"]).is_err());
    }

    #[test]
    fn open_help_exits_without_sending() {
        assert!(matches!(
            run(&["open", "--help"]).unwrap(),
            CliOutcome::Exit { .. }
        ));
    }

    #[test]
    fn log_level_is_passed_through() {
        let CliOutcome::Run { log_level, .. } = run(&["--log-level", "debug"]).unwrap() else {
            panic!()
        };
        assert_eq!(log_level, Some(LogLevel::Debug));
    }
}
