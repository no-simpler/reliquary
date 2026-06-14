# ZDOTDIR-resident .zshenv — safety net for inherited-ZDOTDIR zsh startups.
#
# zsh reads $HOME/.zshenv ONLY when ZDOTDIR is unset at startup. A zsh that
# inherits an already-exported ZDOTDIR (a nested shell, or Claude Code's command
# shell when it is launched from an environment that already set ZDOTDIR) reads
# THIS file instead — so without it, env.d (PATH, tool init, locale, auth env)
# would silently never load. Delegate to the root file so it stays authoritative.
#
# No double-source: a fresh login reads only $HOME/.zshenv (ZDOTDIR unset at
# startup) and does NOT also read this file in the same pass; an inherited-ZDOTDIR
# start reads only this file. The env.d files are idempotent regardless.
source "$HOME/.zshenv"
