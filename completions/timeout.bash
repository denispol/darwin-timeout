# bash completion for darwin-timeout
# copy to /etc/bash_completion.d/ or source in ~/.bashrc

_timeout_completions() {
    local cur prev opts
    COMPREPLY=()
    cur="${COMP_WORDS[COMP_CWORD]}"
    prev="${COMP_WORDS[COMP_CWORD-1]}"

    # Options that take values
    case "$prev" in
        -s|--signal)
            # Common signals
            local signals="TERM HUP INT QUIT KILL USR1 USR2 ALRM STOP CONT"
            COMPREPLY=($(compgen -W "$signals" -- "$cur"))
            return 0
            ;;
        -k|--kill-after|--on-timeout-limit|--wait-for-file-timeout|--retry-delay|-H|--heartbeat|-S|--stdin-timeout)
            # Duration suffixes
            COMPREPLY=($(compgen -W "1s 5s 10s 30s 1m 5m" -- "$cur"))
            return 0
            ;;
        -r|--retry)
            # Common retry counts
            COMPREPLY=($(compgen -W "1 2 3 5 10" -- "$cur"))
            return 0
            ;;
        --retry-backoff)
            # Common backoff multipliers
            COMPREPLY=($(compgen -W "2x 3x 4x" -- "$cur"))
            return 0
            ;;
        --timeout-exit-code)
            COMPREPLY=($(compgen -W "124 125 126 127 0 1" -- "$cur"))
            return 0
            ;;
        --on-timeout)
            # Commands
            COMPREPLY=($(compgen -c -- "$cur"))
            return 0
            ;;
        --wait-for-file)
            # Files
            COMPREPLY=($(compgen -f -- "$cur"))
            return 0
            ;;
        -c|--confine)
            COMPREPLY=($(compgen -W "wall active" -- "$cur"))
            return 0
            ;;
    esac

    # Options
    if [[ "$cur" == -* ]]; then
        opts="-s --signal -k --kill-after -p --preserve-status -f --foreground"
        opts="$opts -v --verbose -q --quiet -c --confine --timeout-exit-code --on-timeout"
        opts="$opts --on-timeout-limit --wait-for-file --wait-for-file-timeout"
        opts="$opts -r --retry --retry-delay --retry-backoff -H --heartbeat -S --stdin-timeout --json -h --help -V --version"
        COMPREPLY=($(compgen -W "$opts" -- "$cur"))
        return 0
    fi

    # After duration, complete commands
    local i cmd_start=0
    for ((i=1; i < COMP_CWORD; i++)); do
        case "${COMP_WORDS[i]}" in
            -s|--signal|-k|--kill-after|--timeout-exit-code|--on-timeout|--on-timeout-limit)
                ((i++))  # skip value
                ;;
            -*)
                ;;
            *)
                if [[ $cmd_start -eq 0 ]]; then
                    cmd_start=$i  # duration
                else
                    # After command, complete files
                    COMPREPLY=($(compgen -f -- "$cur"))
                    return 0
                fi
                ;;
        esac
    done

    # First positional is duration, then command
    if [[ $cmd_start -eq 0 ]]; then
        # Suggest common durations
        COMPREPLY=($(compgen -W "1s 5s 10s 30s 1m 5m 10m 1h" -- "$cur"))
    else
        # Complete commands
        COMPREPLY=($(compgen -c -- "$cur"))
    fi
}

complete -F _timeout_completions timeout
