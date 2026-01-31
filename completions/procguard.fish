# fish completion for procguard
# copy to ~/.config/fish/completions/procguard.fish
# Note: Also symlink to timeout.fish for the timeout alias

# Disable file completion by default
complete -c procguard -f
complete -c timeout -f

# Signals
set -l signals TERM HUP INT QUIT KILL USR1 USR2 ALRM STOP CONT

# Duration suggestions
set -l durations 1s 5s 10s 30s 1m 5m 10m 1h

# Options for procguard
complete -c procguard -s h -l help -d 'Show help message'
complete -c procguard -s V -l version -d 'Show version'
complete -c procguard -s s -l signal -d 'Signal to send on timeout' -xa "$signals"
complete -c procguard -s k -l kill-after -d 'Send KILL after duration' -xa "$durations"
complete -c procguard -s p -l preserve-status -d 'Exit with command status on timeout'
complete -c procguard -s f -l foreground -d 'Run in foreground (allow TTY access)'
complete -c procguard -s v -l verbose -d 'Diagnose signals to stderr'
complete -c procguard -s q -l quiet -d 'Suppress diagnostic output'
complete -c procguard -s c -l confine -d 'Time mode (wall or active)' -xa 'wall active'
complete -c procguard -l timeout-exit-code -d 'Exit code on timeout' -xa '124 125 0 1'
complete -c procguard -l on-timeout -d 'Command to run before signaling' -xa '(__fish_complete_command)'
complete -c procguard -l on-timeout-limit -d 'Timeout for hook command' -xa "$durations"
complete -c procguard -l wait-for-file -d 'Wait for file to exist before starting' -rF
complete -c procguard -l wait-for-file-timeout -d 'Timeout for wait-for-file' -xa "$durations"
complete -c procguard -s r -l retry -d 'Retry command N times on timeout' -xa '1 2 3 5 10'
complete -c procguard -l retry-delay -d 'Delay between retries' -xa "$durations"
complete -c procguard -l retry-backoff -d 'Multiply delay by N each retry' -xa '2x 3x 4x'
complete -c procguard -s H -l heartbeat -d 'Print status to stderr at interval' -xa "$durations"
complete -c procguard -s S -l stdin-timeout -d 'Kill if stdin idle for duration' -xa "$durations"
complete -c procguard -l json -d 'Output JSON for scripting'
complete -c procguard -n '__fish_is_first_arg' -xa "$durations"
complete -c procguard -n 'not __fish_is_first_arg' -xa '(__fish_complete_command)'

# Same options for timeout alias
complete -c timeout -s h -l help -d 'Show help message'
complete -c timeout -s V -l version -d 'Show version'
complete -c timeout -s s -l signal -d 'Signal to send on timeout' -xa "$signals"
complete -c timeout -s k -l kill-after -d 'Send KILL after duration' -xa "$durations"
complete -c timeout -s p -l preserve-status -d 'Exit with command status on timeout'
complete -c timeout -s f -l foreground -d 'Run in foreground (allow TTY access)'
complete -c timeout -s v -l verbose -d 'Diagnose signals to stderr'
complete -c timeout -s q -l quiet -d 'Suppress diagnostic output'
complete -c timeout -s c -l confine -d 'Time mode (wall or active)' -xa 'wall active'
complete -c timeout -l timeout-exit-code -d 'Exit code on timeout' -xa '124 125 0 1'
complete -c timeout -l on-timeout -d 'Command to run before signaling' -xa '(__fish_complete_command)'
complete -c timeout -l on-timeout-limit -d 'Timeout for hook command' -xa "$durations"
complete -c timeout -l wait-for-file -d 'Wait for file to exist before starting' -rF
complete -c timeout -l wait-for-file-timeout -d 'Timeout for wait-for-file' -xa "$durations"
complete -c timeout -s r -l retry -d 'Retry command N times on timeout' -xa '1 2 3 5 10'
complete -c timeout -l retry-delay -d 'Delay between retries' -xa "$durations"
complete -c timeout -l retry-backoff -d 'Multiply delay by N each retry' -xa '2x 3x 4x'
complete -c timeout -s H -l heartbeat -d 'Print status to stderr at interval' -xa "$durations"
complete -c timeout -s S -l stdin-timeout -d 'Kill if stdin idle for duration' -xa "$durations"
complete -c timeout -l json -d 'Output JSON for scripting'
complete -c timeout -n '__fish_is_first_arg' -xa "$durations"
complete -c timeout -n 'not __fish_is_first_arg' -xa '(__fish_complete_command)'
