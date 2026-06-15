#!/bin/bash
#
# Bedrock post-check: once everything is installed, assert the bedrock contract
# holds (bash >=5, python3, uv, docker, git, curl — present, configured,
# PATH-accessible). Loud on a hard miss so a broken bootstrap is obvious, but
# non-fatal — never exits the run. See ~/.config/reliquary/BEDROCK.md.

CHECK_BEDROCK="$HOME/.config/bin/check-bedrock"

if [ ! -x "$CHECK_BEDROCK" ]; then
    print_warning -ad "check-bedrock not found; skipping bedrock verification"
else
    print_bold -ad "Verifying bedrock dependencies..."
    "$CHECK_BEDROCK"
    bedrock_rc=$?
    if [ "$bedrock_rc" -ge 2 ]; then
        print_error -ad "Bedrock INCOMPLETE — see output above (bootstrap continues)"
    elif [ "$bedrock_rc" -eq 1 ]; then
        print_warning -ad "Bedrock satisfied with warnings — see output above"
    else
        print_success -ad "Bedrock satisfied"
    fi
fi
