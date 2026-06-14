##
## PATH priority: force ~/.config/bin ahead of Homebrew (must load LAST)
##
## Highest-numbered env.d file, so this is the final word on $PATH ordering for
## bash and zsh. Earlier work — 040-env's brew shellenv plus its gcloud/cargo/
## OrbStack/gems prepends, and especially a pre-polluted $PATH inherited from a
## GUI launcher (Ghostty, tmux, IDEs, the Claude Code shell) where Homebrew
## already sits ahead — can leave ~/.config/bin behind /opt/homebrew/bin. That
## demotes the `yadm` wrapper (the ~/.config/bin/yadm symlink) behind brew's
## yadm, so bare `yadm` silently bypasses the wrapper. Re-asserting config/bin
## here, after every other prepend, guarantees the wrapper shadows brew's yadm
## in every bash/zsh shell, interactive or not.
##
## Doing this mid-040 was the bug: blocks below it re-won the front. It belongs
## last. (fish has no such bug — 040-env.fish handles its own ordering.)
## Remove-then-prepend is idempotent, so re-sourcing is safe.
##

if [ -d "$HOME/.config/bin" ]; then
    _cb="$HOME/.config/bin"
    _p=":$PATH:"
    _p="${_p//:$_cb:/:}"
    _p="${_p#:}"; _p="${_p%:}"
    export PATH="$_cb:$_p"
    unset _cb _p
fi
