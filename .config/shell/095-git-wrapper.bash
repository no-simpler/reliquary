##
## Git wrapper: disable GPG signing in Claude Code sessions
##

git() {
    if [ "$CLAUDECODE" = "1" ]; then
        case "$1" in
            commit|merge|rebase|cherry-pick|revert|am|tag)
                command git -c commit.gpgsign=false -c tag.gpgsign=false "$@"
                return ;;
        esac
    fi
    command git "$@"
}
