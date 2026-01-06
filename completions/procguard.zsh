#compdef procguard timeout

# zsh completion for procguard
# copy to a directory in your $fpath, e.g., ~/.zsh/completions/
# Note: Works for both 'procguard' and 'timeout' alias

_procguard() {
    local context state state_descr line
    typeset -A opt_args

    local -a signals
    signals=(
        'TERM:terminate process (default)'
        'HUP:hangup'
        'INT:interrupt'
        'QUIT:quit'
        'KILL:kill (cannot be caught)'
        'USR1:user-defined signal 1'
        'USR2:user-defined signal 2'
        'ALRM:alarm'
        'STOP:stop process'
        'CONT:continue process'
    )

    local -a durations
    durations=(
        '1s:one second'
        '5s:five seconds'
        '10s:ten seconds'
        '30s:thirty seconds'
        '1m:one minute'
        '5m:five minutes'
        '10m:ten minutes'
        '1h:one hour'
    )

    _arguments -C \
        '(-h --help)'{-h,--help}'[show help message]' \
        '(-V --version)'{-V,--version}'[show version]' \
        '(-s --signal)'{-s,--signal}'[signal to send on timeout]:signal:->signal' \
        '(-k --kill-after)'{-k,--kill-after}'[send KILL after duration]:duration:->duration' \
        '(-p --preserve-status)'{-p,--preserve-status}'[exit with command status on timeout]' \
        '(-f --foreground)'{-f,--foreground}'[run in foreground (allow TTY access)]' \
        '(-v --verbose -q --quiet)'{-v,--verbose}'[diagnose signals to stderr]' \
        '(-q --quiet -v --verbose)'{-q,--quiet}'[suppress diagnostic output]' \
        '(-c --confine)'{-c,--confine}'[time mode (wall or active)]:mode:(wall active)' \
        '--timeout-exit-code[exit code on timeout]:code:(124 125 0 1)' \
        '--on-timeout[command to run before signaling]:command:_command_names' \
        '--on-timeout-limit[timeout for hook command]:duration:->duration' \
        '--wait-for-file[wait for file to exist before starting]:file:_files' \
        '--wait-for-file-timeout[timeout for wait-for-file]:duration:->duration' \
        '(-r --retry)'{-r,--retry}'[retry command N times on timeout]:count:(1 2 3 5 10)' \
        '--retry-delay[delay between retries]:duration:->duration' \
        '--retry-backoff[multiply delay by N each retry]:multiplier:(2x 3x 4x)' \
        '(-H --heartbeat)'{-H,--heartbeat}'[print status to stderr at interval]:duration:->duration' \
        '(-S --stdin-timeout)'{-S,--stdin-timeout}'[kill if stdin idle for duration]:duration:->duration' \
        '--json[output JSON for scripting]' \
        '1:duration:->duration' \
        '2:command:_command_names' \
        '*:arguments:_files' \
        && return 0

    case "$state" in
        signal)
            _describe -t signals 'signal' signals && return 0
            ;;
        duration)
            _describe -t durations 'duration' durations && return 0
            ;;
    esac

    return 1
}

_procguard "$@"
