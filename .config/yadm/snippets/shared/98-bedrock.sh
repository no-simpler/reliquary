#!/bin/bash
#
# Bedrock post-check: once everything is installed, assert the bedrock contract
# holds (bash >=5, python3, uv, docker, git, curl, just, cargo — present,
# configured, PATH-accessible). Loud on a hard miss so a broken bootstrap is
# obvious, but non-fatal — never exits the run. See ~/.config/reliquary/BEDROCK.md.

if ! command -v assay >/dev/null 2>&1; then
    # Not a skip. 12-publish-relics.sh runs before this, so an absent assay means
    # the publish path is broken — which is a bedrock failure being reported by
    # its own absence, not the absence of a report.
    print_error -ad "assay did not publish — bedrock UNVERIFIED (bootstrap continues)"
else
    print_bold -ad "Verifying bedrock dependencies..."
    assay bedrock
    bedrock_rc=$?
    if [ "$bedrock_rc" -ge 2 ]; then
        print_error -ad "Bedrock INCOMPLETE — see output above (bootstrap continues)"
    elif [ "$bedrock_rc" -eq 1 ]; then
        print_warning -ad "Bedrock satisfied with warnings — see output above"
    else
        print_success -ad "Bedrock satisfied"
    fi
fi
