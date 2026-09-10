## Interactive only
status is-interactive; or return

##
## Prompt: oh-my-posh
##

## ske: publish the open Touch-ID window into $SKE_WINDOW for the oh-my-posh
## `text` segment to render. Mirrors 050-prompt.{zsh,bash}. See the zsh file for
## why this is an env var rather than an oh-my-posh `command` segment (that type
## was removed upstream and silently renders nothing).
##
## fish evaluates fish_prompt for each render, so hooking the event keeps
## $SKE_WINDOW fresh without a precmd equivalent.
function _ske_window --on-event fish_prompt
    set -l sock "$HOME/.local/state/ske/agent.sock"
    test -n "$SKE_STATE"; and set sock "$SKE_STATE/agent.sock"
    if test -S "$sock"
        set -gx SKE_WINDOW (ske prompt 2>/dev/null)
    else
        set -e SKE_WINDOW
    end
end

## coop: draw the card when the outstanding set has moved, and publish the count
## into $COOP_BADGE for the oh-my-posh `text` segment. One exec serves both: the
## card goes to stdout, the count to a file this reads with a builtin.
##
## No status guard here, unlike the zsh and bash twins: fish restores the last
## command's $status for fish_prompt whatever a handler did, which is measured
## rather than assumed.
function _coop_precmd --on-event fish_prompt
    coop tick 2>/dev/null
    set -l badge "$HOME/.local/state/coop/badge"
    test -n "$COOP_ROOT"; and set badge "$COOP_ROOT/badge"
    if test -s "$badge"
        set -gx COOP_BADGE (string collect <"$badge")
    else
        set -e COOP_BADGE
    end
end

## One identity per shell, so a second terminal is shown the card too and
## neither repeats it.
set -gx COOP_SESSION $fish_pid-(random)

if command -q oh-my-posh; and test "$TERM_PROGRAM" != Apple_Terminal
    oh-my-posh init fish --config ~/.config/oh-my-posh/dreamsofautonomy.toml | source
else
    ## Fallback: minimal prompt with color-coded exit status
    function fish_prompt
        set -l last_status $status
        if test $last_status -eq 0
            set_color green
        else
            set_color red
        end
        echo -n (prompt_pwd)' ⋗ '
        set_color normal
    end
end
