//! Runs `mostro-cli` as a subprocess: one fixed binary, arguments passed one
//! by one (never through a shell), a timeout, captured output.

use std::future::Future;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use tokio::process::Command;
use tokio::sync::Mutex;

use crate::config::Config;
use crate::error::{Error, Result};

/// The parent variables the CLI may see besides the node configuration. The
/// child's environment starts empty: an allow-list, so nothing the operator
/// exported for other tools (`ADMIN_NSEC`, `MOSTRO_RPC_TOKEN`, any other
/// secret) can reach it.
pub const FORWARDED_ENV: &[&str] = &[
    "PATH",
    "HOME",
    "USER",
    "TMPDIR",
    "LANG",
    "LC_ALL",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
];

/// Largest stdout or stderr handed back, in bytes: a busy book or a runaway
/// CLI must not flood the agent's context.
pub const MAX_OUTPUT_BYTES: usize = 64 * 1024;

/// Appended where output was cut at [`MAX_OUTPUT_BYTES`].
pub const TRUNCATION_NOTE: &str = "\n[output truncated]";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliOutput {
    pub stdout: String,
    pub stderr: String,
}

/// What the tools need from the CLI: run it with these arguments. The server
/// is generic over it so tests drive the tools without a subprocess.
pub trait CliRunner: Send + Sync + 'static {
    fn run(&self, args: Vec<String>) -> impl Future<Output = Result<CliOutput>> + Send;
}

#[derive(Debug, Clone)]
pub struct MostroCliBackend {
    binary: PathBuf,
    mostro_pubkey: String,
    relays: String,
    pow: Option<u32>,
    transport: Option<String>,
    mcli_dir: Option<PathBuf>,
    timeout: Duration,
    /// One CLI run at a time. Every run opens the same SQLite database and
    /// the next create or take claims the next trade-key index: two at once
    /// could lock the database or put two trades on one key.
    turn: Arc<Mutex<()>>,
}

impl MostroCliBackend {
    pub fn new(config: &Config) -> Self {
        Self {
            binary: config.cli_path.clone(),
            mostro_pubkey: config.mostro_pubkey.clone(),
            relays: config.relays.join(","),
            pow: config.pow,
            transport: config.transport.clone(),
            mcli_dir: config.mcli_dir.clone(),
            timeout: config.timeout,
            turn: Arc::new(Mutex::new(())),
        }
    }

    /// The command for `args`, ready to spawn. Separate so its environment
    /// and arguments are tested without running anything.
    fn command(&self, args: &[String]) -> Command {
        let mut cmd = Command::new(&self.binary);
        cmd.env_clear();
        for key in FORWARDED_ENV {
            if let Some(value) = std::env::var_os(key) {
                cmd.env(key, value);
            }
        }
        cmd.args(args)
            .env("MOSTRO_PUBKEY", &self.mostro_pubkey)
            .env("RELAYS", &self.relays)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // A timed-out run drops the child: this kills it instead of
            // leaving it talking to the relays.
            .kill_on_drop(true);
        if let Some(pow) = self.pow {
            cmd.env("POW", pow.to_string());
        }
        if let Some(transport) = &self.transport {
            cmd.env("TRANSPORT", transport);
        }
        if let Some(dir) = &self.mcli_dir {
            cmd.env("MCLI_DIR", dir);
        }
        cmd
    }
}

impl CliRunner for MostroCliBackend {
    async fn run(&self, args: Vec<String>) -> Result<CliOutput> {
        let _turn = self.turn.lock().await;
        let child = self.command(&args).spawn().map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => Error::CliNotFound(self.binary.display().to_string()),
            _ => Error::Io(e),
        })?;
        let output = tokio::time::timeout(self.timeout, child.wait_with_output())
            .await
            .map_err(|_| Error::Timeout(self.timeout))??;
        let stdout = clean(&output.stdout);
        let stderr = clean(&output.stderr);
        if !output.status.success() {
            return Err(Error::CliFailed {
                status: output.status.to_string(),
                stderr,
                stdout,
            });
        }
        Ok(CliOutput { stdout, stderr })
    }
}

fn clean(bytes: &[u8]) -> String {
    truncate(redact_secrets(&strip_ansi(&String::from_utf8_lossy(bytes))))
}

/// Cuts `text` to at most [`MAX_OUTPUT_BYTES`] on a character boundary and
/// says so.
fn truncate(mut text: String) -> String {
    if text.len() <= MAX_OUTPUT_BYTES {
        return text;
    }
    let mut cut = MAX_OUTPUT_BYTES;
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    text.truncate(cut);
    text.push_str(TRUNCATION_NOTE);
    text
}

/// Removes ANSI escape sequences (colours, cursor moves) from CLI output.
pub fn strip_ansi(input: &str) -> String {
    const ESC: char = '\u{1b}';
    const BEL: char = '\u{7}';
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c != ESC {
            out.push(c);
            continue;
        }
        match chars.next() {
            // CSI: parameters, then one final byte in @..~.
            Some('[') => {
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            }
            // OSC: up to BEL or ST (ESC \).
            Some(']') => {
                while let Some(c) = chars.next() {
                    if c == BEL {
                        break;
                    }
                    if c == ESC {
                        chars.next();
                        break;
                    }
                }
            }
            // Any other escape is two characters long.
            _ => {}
        }
    }
    out
}

/// Characters after `nsec1` from which a run is treated as a key: a real one
/// has 58, a word that merely starts with `nsec1` far fewer.
const MIN_NSEC_BODY: usize = 50;

/// Replaces anything shaped like a Nostr secret key (`nsec1…`) with a
/// placeholder. The allow-listed commands print no secrets; this is the
/// second line, so a future CLI change cannot leak one to the agent.
pub fn redact_secrets(input: &str) -> String {
    const PREFIX: &str = "nsec1";
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(pos) = rest.find(PREFIX) {
        out.push_str(&rest[..pos]);
        let body = &rest[pos + PREFIX.len()..];
        let len = body
            .find(|c: char| !c.is_ascii_alphanumeric())
            .unwrap_or(body.len());
        if len >= MIN_NSEC_BODY {
            out.push_str("[redacted nsec]");
        } else {
            out.push_str(&rest[pos..pos + PREFIX.len() + len]);
        }
        rest = &body[len..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {

    use super::*;

    fn backend(binary: PathBuf, timeout: Duration) -> MostroCliBackend {
        MostroCliBackend::new(&Config {
            cli_path: binary,
            mostro_pubkey: "ab".repeat(32),
            relays: vec!["ws://localhost:7000".into()],
            pow: Some(0),
            transport: None,
            mcli_dir: Some(PathBuf::from("/tmp/mcli-test")),
            allow_writes: false,
            timeout,
        })
    }

    /// A stand-in CLI: a shell script, so the tests need no mostro-cli.
    ///
    /// A child process writes it. Written from this process, the file would
    /// be open for writing while a parallel test forks; the fork inherits
    /// that descriptor, and executing the script then fails with "Text file
    /// busy" (ETXTBSY).
    fn script(dir: &tempfile::TempDir, body: &str) -> PathBuf {
        let path = dir.path().join("fake-mostro-cli");
        let status = std::process::Command::new("/bin/sh")
            .args(["-c", r#"printf '%s\n' "$1" > "$2" && chmod 755 "$2""#, "sh"])
            .arg(format!("#!/bin/sh\n{body}"))
            .arg(&path)
            .status()
            .unwrap();
        assert!(status.success(), "could not write the fake CLI");
        path
    }

    #[test]
    fn the_command_carries_the_node_config_and_nothing_else_from_the_parent() {
        let cmd = backend(PathBuf::from("/bin/true"), Duration::from_secs(1))
            .command(&["listorders".into()]);
        let envs: Vec<(String, Option<String>)> = cmd
            .as_std()
            .get_envs()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().into_owned(),
                    v.map(|v| v.to_string_lossy().into_owned()),
                )
            })
            .collect();
        assert!(envs.contains(&("MOSTRO_PUBKEY".into(), Some("ab".repeat(32)))));
        assert!(envs.contains(&("RELAYS".into(), Some("ws://localhost:7000".into()))));
        assert!(envs.contains(&("POW".into(), Some("0".into()))));
        assert!(envs.contains(&("MCLI_DIR".into(), Some("/tmp/mcli-test".into()))));
        // The environment is cleared first: only these names are ever set.
        let set_here = ["MOSTRO_PUBKEY", "RELAYS", "POW", "TRANSPORT", "MCLI_DIR"];
        for (key, _) in &envs {
            assert!(
                set_here.contains(&key.as_str()) || FORWARDED_ENV.contains(&key.as_str()),
                "{key} reaches the CLI"
            );
        }
        let args: Vec<_> = cmd.as_std().get_args().collect();
        assert_eq!(args, ["listorders"]);
    }

    #[tokio::test]
    async fn arguments_reach_the_cli_verbatim_without_a_shell() {
        let dir = tempfile::tempdir().unwrap();
        let cli = script(&dir, r#"for a in "$@"; do printf '%s\n' "$a"; done"#);
        let hostile = vec![
            "--paymentmethod=; rm -rf ~".to_string(),
            "$(whoami)".to_string(),
            "`id`".to_string(),
        ];
        let out = backend(cli, Duration::from_secs(5))
            .run(hostile.clone())
            .await
            .unwrap();
        assert_eq!(out.stdout.lines().collect::<Vec<_>>(), hostile);
    }

    #[tokio::test]
    async fn a_missing_binary_says_where_it_looked() {
        let err = backend(
            PathBuf::from("/nonexistent/mostro-cli"),
            Duration::from_secs(1),
        )
        .run(vec![])
        .await
        .unwrap_err();
        assert!(matches!(err, Error::CliNotFound(path) if path.contains("/nonexistent")));
    }

    #[tokio::test]
    async fn a_failing_cli_reports_its_status_and_stderr() {
        let dir = tempfile::tempdir().unwrap();
        let cli = script(&dir, "echo 'MOSTRO_PUBKEY not set' >&2; exit 1");
        let err = backend(cli, Duration::from_secs(5))
            .run(vec![])
            .await
            .unwrap_err();
        match err {
            Error::CliFailed { status, stderr, .. } => {
                assert!(status.contains('1'), "{status}");
                assert!(stderr.contains("MOSTRO_PUBKEY not set"));
            }
            other => panic!("expected CliFailed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_hung_cli_times_out() {
        let dir = tempfile::tempdir().unwrap();
        let cli = script(&dir, "sleep 30");
        let err = backend(cli, Duration::from_millis(200))
            .run(vec![])
            .await
            .unwrap_err();
        assert!(matches!(err, Error::Timeout(_)));
    }

    #[tokio::test]
    async fn a_timeout_warns_that_the_command_may_have_reached_the_node() {
        let dir = tempfile::tempdir().unwrap();
        let cli = script(&dir, "sleep 30");
        let err = backend(cli, Duration::from_millis(200))
            .run(vec![])
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("may have reached the node"),
            "{err}"
        );
    }

    #[tokio::test]
    async fn runs_never_overlap_so_the_cli_database_is_used_by_one_at_a_time() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("log");
        let cli = script(&dir, r#"echo start >> "$1"; sleep 0.3; echo end >> "$1""#);
        let backend = backend(cli, Duration::from_secs(5));
        let arg = vec![log.display().to_string()];
        let (first, second) = tokio::join!(backend.run(arg.clone()), backend.run(arg));
        first.unwrap();
        second.unwrap();
        assert_eq!(
            std::fs::read_to_string(&log).unwrap(),
            "start\nend\nstart\nend\n"
        );
    }

    #[tokio::test]
    async fn oversized_output_is_truncated() {
        let dir = tempfile::tempdir().unwrap();
        let cli = script(&dir, r"head -c 200000 /dev/zero | tr '\0' x");
        let out = backend(cli, Duration::from_secs(5))
            .run(vec![])
            .await
            .unwrap();
        assert!(out.stdout.len() <= MAX_OUTPUT_BYTES + TRUNCATION_NOTE.len());
        assert!(out.stdout.ends_with(TRUNCATION_NOTE));
    }

    #[tokio::test]
    async fn output_is_cleaned_before_it_reaches_the_agent() {
        let dir = tempfile::tempdir().unwrap();
        let nsec = format!("nsec1{}", "q".repeat(58));
        let cli = script(
            &dir,
            &format!(r"printf '\033[1;32mOrder created\033[0m {nsec}\n'"),
        );
        let out = backend(cli, Duration::from_secs(5))
            .run(vec![])
            .await
            .unwrap();
        assert_eq!(out.stdout.trim(), "Order created [redacted nsec]");
    }

    #[test]
    fn strip_ansi_keeps_text_and_drops_sequences() {
        assert_eq!(strip_ansi("\u{1b}[31mred\u{1b}[0m plain"), "red plain");
        assert_eq!(strip_ansi("📋 List │ ok"), "📋 List │ ok");
    }

    #[test]
    fn redact_secrets_replaces_every_nsec() {
        let key = format!("nsec1{}", "a".repeat(58));
        let text = format!("a {key} b {key}");
        assert_eq!(redact_secrets(&text), "a [redacted nsec] b [redacted nsec]");
        assert_eq!(redact_secrets("npub1abc stays"), "npub1abc stays");
    }
}
