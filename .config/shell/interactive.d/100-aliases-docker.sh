##
## Docker
##

alias dcu='docker compose up -d'
alias dcb='docker compose up -d --build --no-deps'
alias dce='docker compose exec'
alias dcd='docker compose down'
alias dcdd='docker compose down --volumes --rmi all --remove-orphans'
alias dcl='docker compose logs -f'

dcex() {
    docker compose exec \
        --env XDEBUG_TRIGGER=1 \
        --env PHP_IDE_CONFIG="serverName=$(basename "$PWD")" \
        "$@"
}
