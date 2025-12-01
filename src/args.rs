/*
 * args.rs
 *
 * Clap derive macros handle parsing. Life's too short to do this by hand.
 *
 * GNU compatible: scripts written for GNU timeout should just work.
 * We add --json but don't change existing flags.
 *
 * trailing_var_arg grabs everything after COMMAND so `timeout 5s grep -r`
 * doesn't try to parse grep's flags.
 */

use clap::CommandFactory;
use clap::Parser;
use clap_complete::Shell;
use std::io;

#[derive(Parser, Debug)]
#[command(
    name = "timeout",
    version,
    about = "Run a command with a time limit",
    long_about = "Start COMMAND, and kill it if still running after DURATION.\n\n\
                  DURATION is a floating-point number with optional suffix:\n\
                  's' for seconds (default), 'm' for minutes, 'h' for hours, 'd' for days.\n\n\
                  Examples:\n\
                    timeout 30 cmd        # 30 seconds\n\
                    timeout 30s cmd       # 30 seconds (explicit)\n\
                    timeout 1.5m cmd      # 1.5 minutes (90 seconds)\n\
                    timeout 2h cmd        # 2 hours\n\n\
                  A duration of 0 disables the timeout.\n\n\
                  If the command times out, and --preserve-status is not set, exit with status 124.\n\
                  Otherwise, exit with the status of COMMAND.\n\n\
                  If no signal is specified, SIGTERM is sent. If SIGTERM fails to kill the process,\n\
                  consider using SIGKILL (9), or use --kill-after to send SIGKILL after a delay.",
    after_help = "Exit status:\n\
                  124 if COMMAND times out, and --preserve-status is not specified\n\
                  125 if the timeout command itself fails\n\
                  126 if COMMAND is found but cannot be invoked\n\
                  127 if COMMAND cannot be found\n\
                  137 if COMMAND (or timeout itself) is sent SIGKILL (128+9)\n\
                  the exit status of COMMAND otherwise"
)]
pub struct Args {
    /// Output result as JSON (for scripting/CI).
    ///
    /// Prints a JSON object with status, signal, elapsed_ms, and exit_code.
    #[arg(long = "json")]
    pub json: bool,

    /// Generate shell completions and exit.
    ///
    /// Outputs completion script for the specified shell to stdout.
    /// Supported: bash, zsh, fish, powershell, elvish.
    #[arg(long = "completions", value_name = "SHELL")]
    pub completions: Option<Shell>,

    /// Specify the signal to be sent on timeout.
    ///
    /// SIGNAL may be a name like 'TERM', 'HUP', or 'KILL', or a number.
    /// See 'kill -l' for a list of signals.
    /// Falls back to TIMEOUT_SIGNAL environment variable.
    #[arg(
        short = 's',
        long = "signal",
        default_value = "TERM",
        value_name = "SIGNAL",
        env = "TIMEOUT_SIGNAL"
    )]
    pub signal: String,

    /// Send a KILL signal if COMMAND is still running after DURATION.
    ///
    /// This ensures the process is killed after the specified additional time,
    /// even if it ignores the initial signal.
    /// Falls back to TIMEOUT_KILL_AFTER environment variable.
    #[arg(
        short = 'k',
        long = "kill-after",
        value_name = "DURATION",
        env = "TIMEOUT_KILL_AFTER"
    )]
    pub kill_after: Option<String>,

    /// Exit with the same status as COMMAND, even when the command times out.
    ///
    /// Without this option, exit with status 124 on timeout.
    #[arg(short = 'p', long = "preserve-status")]
    pub preserve_status: bool,

    /// Allow COMMAND to read from the TTY and get TTY signals.
    ///
    /// In this mode, children of COMMAND will not be timed out.
    /// Without this option, the command runs in a separate process group.
    #[arg(short = 'f', long = "foreground")]
    pub foreground: bool,

    /// Diagnose to stderr any signal sent upon timeout.
    #[arg(short = 'v', long = "verbose")]
    pub verbose: bool,

    /// Suppress timeout's own diagnostic output to stderr.
    ///
    /// Error messages and verbose output are not printed.
    /// The command's own stderr is still passed through.
    /// Mutually exclusive with --verbose.
    #[arg(short = 'q', long = "quiet", conflicts_with = "verbose")]
    pub quiet: bool,

    /// Exit with CODE instead of 124 when the command times out.
    ///
    /// Useful for distinguishing timeout from other exit codes in scripts.
    #[arg(long = "timeout-exit-code", value_name = "CODE")]
    pub timeout_exit_code: Option<u8>,

    /// Run CMD before sending the timeout signal.
    ///
    /// Use %p in CMD to substitute the process ID. Useful for capturing
    /// diagnostics (thread dumps, heap snapshots) before killing.
    /// The hook has 5 seconds to complete by default.
    #[arg(long = "on-timeout", value_name = "CMD")]
    pub on_timeout: Option<String>,

    /// Timeout for the --on-timeout hook command (default: 5s).
    #[arg(
        long = "on-timeout-limit",
        value_name = "DURATION",
        default_value = "5s"
    )]
    pub on_timeout_limit: String,

    /// Duration before sending signal.
    ///
    /// A floating-point number with optional suffix:
    /// 's' for seconds (default), 'm' for minutes, 'h' for hours, 'd' for days.
    /// A duration of 0 disables the timeout.
    ///
    /// Falls back to TIMEOUT environment variable if not provided.
    #[arg(value_name = "DURATION")]
    pub duration: Option<String>,

    /// Command to run.
    #[arg(value_name = "COMMAND", allow_hyphen_values = true)]
    pub command: Option<String>,

    /// Arguments for the command.
    #[arg(
        trailing_var_arg = true,
        allow_hyphen_values = true,
        value_name = "ARG"
    )]
    pub args: Vec<String>,
}

impl Args {
    #[must_use]
    pub fn parse_args() -> Self {
        Self::parse()
    }

    /// generate shell completions to stdout
    pub fn print_completions(shell: Shell) {
        let mut cmd = Self::command();
        clap_complete::generate(shell, &mut cmd, "timeout", &mut io::stdout());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minimal_args() {
        let args = Args::try_parse_from(["timeout", "5", "sleep", "10"]).unwrap();
        assert_eq!(args.duration, Some("5".to_string()));
        assert_eq!(args.command, Some("sleep".to_string()));
        assert_eq!(args.args, vec!["10"]);
        assert_eq!(args.signal, "TERM");
        assert!(!args.preserve_status);
        assert!(!args.foreground);
        assert!(!args.verbose);
        assert!(!args.quiet);
        assert!(args.kill_after.is_none());
        assert!(!args.json);
        assert!(args.timeout_exit_code.is_none());
        assert!(args.on_timeout.is_none());
    }

    #[test]
    fn test_all_options() {
        /* Note: -v and -q now conflict, so we test them separately */
        let args = Args::try_parse_from([
            "timeout",
            "-s",
            "KILL",
            "-k",
            "5s",
            "-p",
            "-f",
            "-v",
            "--timeout-exit-code",
            "42",
            "--on-timeout",
            "echo %p",
            "30s",
            "my_command",
            "arg1",
            "arg2",
        ])
        .unwrap();

        assert_eq!(args.signal, "KILL");
        assert_eq!(args.kill_after, Some("5s".to_string()));
        assert!(args.preserve_status);
        assert!(args.foreground);
        assert!(args.verbose);
        assert!(!args.quiet);
        assert_eq!(args.timeout_exit_code, Some(42));
        assert_eq!(args.on_timeout, Some("echo %p".to_string()));
        assert_eq!(args.duration, Some("30s".to_string()));
        assert_eq!(args.command, Some("my_command".to_string()));
        assert_eq!(args.args, vec!["arg1", "arg2"]);
    }

    #[test]
    fn test_long_options() {
        let args = Args::try_parse_from([
            "timeout",
            "--signal=HUP",
            "--kill-after=10m",
            "--preserve-status",
            "--foreground",
            "--verbose",
            "1h",
            "cmd",
        ])
        .unwrap();

        assert_eq!(args.signal, "HUP");
        assert_eq!(args.kill_after, Some("10m".to_string()));
        assert!(args.preserve_status);
        assert!(args.foreground);
        assert!(args.verbose);
        assert!(!args.quiet);
        assert_eq!(args.duration, Some("1h".to_string()));
        assert_eq!(args.command, Some("cmd".to_string()));
    }

    #[test]
    fn test_quiet_verbose_conflict() {
        /* -q and -v should conflict */
        let result = Args::try_parse_from(["timeout", "-q", "-v", "5s", "cmd"]);
        assert!(result.is_err(), "-q and -v should be mutually exclusive");
    }

    #[test]
    fn test_command_with_dashes() {
        /* Commands starting with - need the -- separator */
        let args = Args::try_parse_from(["timeout", "5", "--", "-c", "echo", "hello"]).unwrap();
        assert_eq!(args.command, Some("-c".to_string()));
        assert_eq!(args.args, vec!["echo", "hello"]);
    }

    #[test]
    fn test_json_flag() {
        let args = Args::try_parse_from(["timeout", "--json", "5s", "sleep", "1"]).unwrap();
        assert!(args.json);
        assert_eq!(args.duration, Some("5s".to_string()));
    }

    #[test]
    fn test_quiet_flag() {
        let args = Args::try_parse_from(["timeout", "-q", "5s", "cmd"]).unwrap();
        assert!(args.quiet);
    }

    #[test]
    fn test_timeout_exit_code() {
        let args =
            Args::try_parse_from(["timeout", "--timeout-exit-code", "99", "5s", "cmd"]).unwrap();
        assert_eq!(args.timeout_exit_code, Some(99));
    }

    #[test]
    fn test_on_timeout() {
        let args =
            Args::try_parse_from(["timeout", "--on-timeout", "echo %p", "5s", "cmd"]).unwrap();
        assert_eq!(args.on_timeout, Some("echo %p".to_string()));
    }
}
