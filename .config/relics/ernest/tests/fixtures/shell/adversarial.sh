#!/usr/bin/env bash
#
# Every `#` below that is not a comment, and every comment sigil the house
# style uses. Lifted from real scripts under ~/.config.

set -euo pipefail

## Section header — em-dash and ── box drawing, so the count stays per-character
#>  mcd PATH
#.  -a  - Include names starting with dots
# --- Colors (house style) --------------------------------------------------

# shellcheck disable=SC1091
source "$HOME/.config/reliquary/lib/relic.sh"

base="${PWD##*/}"
trimmed="${line#"${line%%[![:space:]]*}"}"
home="${XDG_DATA/#$HOME/\~}"
count=${#REPOS[@]}

while [[ $# -gt 0 ]]; do
    shift
done

case "$1" in
    '#'*|'') printf 'skipped\n' ;;
    \#*)     printf 'also skipped\n' ;;
    *)       printf 'bash\n' ;;   # /bin/sh, /usr/bin/env sh, sh -e → bash
esac

echo "value # not a comment"
echo 'value # not a comment either'
awk '/^[[:space:]]*#/ { next } { print }' "$REGISTRY"
printf '#!/usr/bin/env bash\necho hi\n' > "$out"
echo # move to a new line

cat > "$hook" <<EOF
#!/bin/bash
# Nested script, expanded: still the payload, not prose.
exec "$target" "\$@"
EOF

cat > "$plist" <<'EOT'
#!/bin/bash
# Nested script, quoted: likewise.
EOT
