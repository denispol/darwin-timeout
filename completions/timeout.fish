# fish completion for darwin-timeout
# copy to ~/.config/fish/completions/timeout.fish

# Disable file completion by default
complete -c timeout -f

# Signals
set -l signals TERM HUP INT QUIT KILL USR1 USR2 ALRM STOP CONT

# Duration suggestions
set -l durations 1s 5s 10s 30s 1m 5m 10m 1h

# Options
complete -c timeout -s h -l help -d 'Show help message'
complete -c timeout -s V -l version -d 'Show version'
complete -c timeout -s s -l signal -d 'Signal to send on timeout' -xa "$signals"
complete -c timeout -s k -l kill-after -d 'Send KILL after duration' -xa "$durations"
complete -c timeout -s p -l preserve-status -d 'Exit with command status on timeout'
complete -c timeout -s f -l foreground -d 'Run in foreground (allow TTY access)'
complete -c timeout -s v -l verbose -d 'Diagnose signals to stderr'
complete -c timeout -s q -l quiet -d 'Suppress diagnostic output'
complete -c timeout -l timeout-exit-code -d 'Exit code on timeout' -xa '124 125 0 1'
complete -c timeout -l on-timeout -d 'Command to run before signaling' -xa '(__fish_complete_command)'
complete -c timeout -l on-timeout-limit -d 'Timeout for hook command' -xa "$durations"
complete -c timeout -l json -d 'Output JSON for scripting'

# First positional argument: duration
complete -c timeout -n '__fish_is_first_arg' -xa "$durations"

# After duration: complete commands
complete -c timeout -n 'not __fish_is_first_arg' -xa '(__fish_complete_command)'
