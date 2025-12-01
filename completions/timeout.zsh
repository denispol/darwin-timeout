#compdef timeout

# zsh completion for darwin-timeout
# copy to a directory in your $fpath, e.g., ~/.zsh/completions/

_timeout() {
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
        '--timeout-exit-code[exit code on timeout]:code:(124 125 0 1)' \
        '--on-timeout[command to run before signaling]:command:_command_names' \
        '--on-timeout-limit[timeout for hook command]:duration:->duration' \
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

_timeout "$@"
