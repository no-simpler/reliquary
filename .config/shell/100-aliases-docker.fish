##
## Docker
##

abbr --add dcu 'docker compose up -d'
abbr --add dcb 'docker compose up -d --build --no-deps'
abbr --add dce 'docker compose exec'
abbr --add dcd 'docker compose down'
abbr --add dcdd 'docker compose down --volumes --rmi all --remove-orphans'
abbr --add dcl 'docker compose logs -f'

function dcex
    docker compose exec \
        --env XDEBUG_TRIGGER=1 \
        --env PHP_IDE_CONFIG="serverName="(basename $PWD) \
        $argv
end
