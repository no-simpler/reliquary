##
## Fuzzy find — key bindings + completion for the current shell
##
## fzf ships per-shell init (`fzf --bash` / `fzf --zsh`); $D__SHELL selects
## which to source so bash and zsh both get the CTRL-T / CTRL-R / ALT-C widgets.
## fish gets its bindings from the fzf.fish Fisher plugin instead
## (see fish/conf.d/fzf.fish), so this shared file is bash + zsh only.
##

if command -v fzf >/dev/null 2>&1; then
    case "$D__SHELL" in
    bash | zsh) source <(fzf --"$D__SHELL") ;;
    esac
fi
