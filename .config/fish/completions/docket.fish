# Print an optspec for argparse to handle cmd's options that are independent of any subcommand.
function __fish_docket_global_optspecs
    string join \n format= json color= project= q/quiet h/help V/version
end

function __fish_docket_needs_command
    # Figure out if the current invocation already has a command.
    set -l cmd (commandline -opc)
    set -e cmd[1]
    argparse -s (__fish_docket_global_optspecs) -- $cmd 2>/dev/null
    or return
    if set -q argv[1]
        # Also print the command, so this can be used to figure out what it is.
        echo $argv[1]
        return 1
    end
    return 0
end

function __fish_docket_using_subcommand
    set -l cmd (__fish_docket_needs_command)
    test -z "$cmd"
    and return 1
    contains -- $cmd[1] $argv
end

complete -c docket -n "__fish_docket_needs_command" -l format -d 'Defaults to agent under Claude Code or off a terminal, human otherwise' -r -f -a "human\t''
agent\t''
json\t''"
complete -c docket -n "__fish_docket_needs_command" -l color -d 'Honours NO_COLOR and CLICOLOR_FORCE' -r -f -a "auto\t''
always\t''
never\t''"
complete -c docket -n "__fish_docket_needs_command" -l project -d 'Act on another project\'s docket' -r -F
complete -c docket -n "__fish_docket_needs_command" -l json -d 'Shorthand for --format json'
complete -c docket -n "__fish_docket_needs_command" -s q -l quiet -d 'Print only what was asked for'
complete -c docket -n "__fish_docket_needs_command" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c docket -n "__fish_docket_needs_command" -s V -l version -d 'Print version'
complete -c docket -n "__fish_docket_needs_command" -f -a "list" -d 'List outstanding items'
complete -c docket -n "__fish_docket_needs_command" -f -a "ls" -d 'List outstanding items'
complete -c docket -n "__fish_docket_needs_command" -f -a "create" -d 'New docket item; returns path'
complete -c docket -n "__fish_docket_needs_command" -f -a "show" -d 'Print an item\'s body'
complete -c docket -n "__fish_docket_needs_command" -f -a "path" -d 'Print an item\'s file path, for writing or editing its body'
complete -c docket -n "__fish_docket_needs_command" -f -a "set" -d 'Edit docket item metadata'
complete -c docket -n "__fish_docket_needs_command" -f -a "reorder" -d 'Change order of docket items'
complete -c docket -n "__fish_docket_needs_command" -f -a "promote" -d 'Advance docket item kind'
complete -c docket -n "__fish_docket_needs_command" -f -a "relay" -d 'Replace relay with successor'
complete -c docket -n "__fish_docket_needs_command" -f -a "close" -d 'Close a docket item whose work is done'
complete -c docket -n "__fish_docket_needs_command" -f -a "doctor" -d 'Report invalid metadata for fixing'
complete -c docket -n "__fish_docket_needs_command" -f -a "announce" -d 'Emit banner of outstanding work, if any'
complete -c docket -n "__fish_docket_needs_command" -f -a "help" -d 'Explain a topic, or a command'
complete -c docket -n "__fish_docket_needs_command" -f -a "guide" -d 'Doctrine: what to write, and when. Name kinds to append their guidance'
complete -c docket -n "__fish_docket_needs_command" -f -a "completions" -d 'Print a shell completion script'
complete -c docket -n "__fish_docket_using_subcommand list" -l kind -d 'Only this kind' -r -f -a "handoff\t''
relay\t''
spec\t''"
complete -c docket -n "__fish_docket_using_subcommand list" -l format -d 'Defaults to agent under Claude Code or off a terminal, human otherwise' -r -f -a "human\t''
agent\t''
json\t''"
complete -c docket -n "__fish_docket_using_subcommand list" -l color -d 'Honours NO_COLOR and CLICOLOR_FORCE' -r -f -a "auto\t''
always\t''
never\t''"
complete -c docket -n "__fish_docket_using_subcommand list" -l project -d 'Act on another project\'s docket' -r -F
complete -c docket -n "__fish_docket_using_subcommand list" -l all -d 'Every project on this machine, not just this one'
complete -c docket -n "__fish_docket_using_subcommand list" -l blocked -d 'Only items carrying a block'
complete -c docket -n "__fish_docket_using_subcommand list" -l invalid -d 'Only items whose metadata will not parse'
complete -c docket -n "__fish_docket_using_subcommand list" -l json -d 'Shorthand for --format json'
complete -c docket -n "__fish_docket_using_subcommand list" -s q -l quiet -d 'Print only what was asked for'
complete -c docket -n "__fish_docket_using_subcommand list" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c docket -n "__fish_docket_using_subcommand ls" -l kind -d 'Only this kind' -r -f -a "handoff\t''
relay\t''
spec\t''"
complete -c docket -n "__fish_docket_using_subcommand ls" -l format -d 'Defaults to agent under Claude Code or off a terminal, human otherwise' -r -f -a "human\t''
agent\t''
json\t''"
complete -c docket -n "__fish_docket_using_subcommand ls" -l color -d 'Honours NO_COLOR and CLICOLOR_FORCE' -r -f -a "auto\t''
always\t''
never\t''"
complete -c docket -n "__fish_docket_using_subcommand ls" -l project -d 'Act on another project\'s docket' -r -F
complete -c docket -n "__fish_docket_using_subcommand ls" -l all -d 'Every project on this machine, not just this one'
complete -c docket -n "__fish_docket_using_subcommand ls" -l blocked -d 'Only items carrying a block'
complete -c docket -n "__fish_docket_using_subcommand ls" -l invalid -d 'Only items whose metadata will not parse'
complete -c docket -n "__fish_docket_using_subcommand ls" -l json -d 'Shorthand for --format json'
complete -c docket -n "__fish_docket_using_subcommand ls" -s q -l quiet -d 'Print only what was asked for'
complete -c docket -n "__fish_docket_using_subcommand ls" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c docket -n "__fish_docket_using_subcommand create" -l name -d 'Up to three words of A-Z, 0-9 and underscore, at most 20 characters. Case and separators are normalised, so rosetta-messenger stores as ROSETTA_MESSENGER' -r
complete -c docket -n "__fish_docket_using_subcommand create" -l tagline -d 'One line under the name, at most 80 characters. Use - for standard input' -r
complete -c docket -n "__fish_docket_using_subcommand create" -l to -d 'Open it for another project. Defaults to this one' -r -F
complete -c docket -n "__fish_docket_using_subcommand create" -l body -d 'Body to write, instead of leaving the file for you to fill in. Use - for standard input' -r
complete -c docket -n "__fish_docket_using_subcommand create" -l format -d 'Defaults to agent under Claude Code or off a terminal, human otherwise' -r -f -a "human\t''
agent\t''
json\t''"
complete -c docket -n "__fish_docket_using_subcommand create" -l color -d 'Honours NO_COLOR and CLICOLOR_FORCE' -r -f -a "auto\t''
always\t''
never\t''"
complete -c docket -n "__fish_docket_using_subcommand create" -l project -d 'Act on another project\'s docket' -r -F
complete -c docket -n "__fish_docket_using_subcommand create" -l allow-missing -d 'Allow a target directory that does not exist yet'
complete -c docket -n "__fish_docket_using_subcommand create" -l json -d 'Shorthand for --format json'
complete -c docket -n "__fish_docket_using_subcommand create" -s q -l quiet -d 'Print only what was asked for'
complete -c docket -n "__fish_docket_using_subcommand create" -s h -l help -d 'Print help'
complete -c docket -n "__fish_docket_using_subcommand show" -l format -d 'Defaults to agent under Claude Code or off a terminal, human otherwise' -r -f -a "human\t''
agent\t''
json\t''"
complete -c docket -n "__fish_docket_using_subcommand show" -l color -d 'Honours NO_COLOR and CLICOLOR_FORCE' -r -f -a "auto\t''
always\t''
never\t''"
complete -c docket -n "__fish_docket_using_subcommand show" -l project -d 'Act on another project\'s docket' -r -F
complete -c docket -n "__fish_docket_using_subcommand show" -l json -d 'Shorthand for --format json'
complete -c docket -n "__fish_docket_using_subcommand show" -s q -l quiet -d 'Print only what was asked for'
complete -c docket -n "__fish_docket_using_subcommand show" -s h -l help -d 'Print help'
complete -c docket -n "__fish_docket_using_subcommand path" -l format -d 'Defaults to agent under Claude Code or off a terminal, human otherwise' -r -f -a "human\t''
agent\t''
json\t''"
complete -c docket -n "__fish_docket_using_subcommand path" -l color -d 'Honours NO_COLOR and CLICOLOR_FORCE' -r -f -a "auto\t''
always\t''
never\t''"
complete -c docket -n "__fish_docket_using_subcommand path" -l project -d 'Act on another project\'s docket' -r -F
complete -c docket -n "__fish_docket_using_subcommand path" -l json -d 'Shorthand for --format json'
complete -c docket -n "__fish_docket_using_subcommand path" -s q -l quiet -d 'Print only what was asked for'
complete -c docket -n "__fish_docket_using_subcommand path" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c docket -n "__fish_docket_using_subcommand set" -l name -d 'Replace the name' -r
complete -c docket -n "__fish_docket_using_subcommand set" -l tagline -d 'Replace the tagline. Use - for standard input' -r
complete -c docket -n "__fish_docket_using_subcommand set" -l blocked -d 'Record what must clear before this item can move, in one line. Use - for standard input' -r
complete -c docket -n "__fish_docket_using_subcommand set" -l tags -d 'Replace the tags wholesale' -r
complete -c docket -n "__fish_docket_using_subcommand set" -l format -d 'Defaults to agent under Claude Code or off a terminal, human otherwise' -r -f -a "human\t''
agent\t''
json\t''"
complete -c docket -n "__fish_docket_using_subcommand set" -l color -d 'Honours NO_COLOR and CLICOLOR_FORCE' -r -f -a "auto\t''
always\t''
never\t''"
complete -c docket -n "__fish_docket_using_subcommand set" -l project -d 'Act on another project\'s docket' -r -F
complete -c docket -n "__fish_docket_using_subcommand set" -l clear-blocked -d 'Drop the block'
complete -c docket -n "__fish_docket_using_subcommand set" -l json -d 'Shorthand for --format json'
complete -c docket -n "__fish_docket_using_subcommand set" -s q -l quiet -d 'Print only what was asked for'
complete -c docket -n "__fish_docket_using_subcommand set" -s h -l help -d 'Print help'
complete -c docket -n "__fish_docket_using_subcommand reorder" -l position -d 'Move it to this position, counting from one' -r
complete -c docket -n "__fish_docket_using_subcommand reorder" -l before -d 'Move it directly ahead of this item' -r
complete -c docket -n "__fish_docket_using_subcommand reorder" -l after -d 'Move it directly behind this item' -r
complete -c docket -n "__fish_docket_using_subcommand reorder" -l sequence -d 'Reorder in bulk. Listed items move to the front in this order' -r
complete -c docket -n "__fish_docket_using_subcommand reorder" -l format -d 'Defaults to agent under Claude Code or off a terminal, human otherwise' -r -f -a "human\t''
agent\t''
json\t''"
complete -c docket -n "__fish_docket_using_subcommand reorder" -l color -d 'Honours NO_COLOR and CLICOLOR_FORCE' -r -f -a "auto\t''
always\t''
never\t''"
complete -c docket -n "__fish_docket_using_subcommand reorder" -l project -d 'Act on another project\'s docket' -r -F
complete -c docket -n "__fish_docket_using_subcommand reorder" -l top -d 'Move it to the front'
complete -c docket -n "__fish_docket_using_subcommand reorder" -l bottom -d 'Move it to the back'
complete -c docket -n "__fish_docket_using_subcommand reorder" -l json -d 'Shorthand for --format json'
complete -c docket -n "__fish_docket_using_subcommand reorder" -s q -l quiet -d 'Print only what was asked for'
complete -c docket -n "__fish_docket_using_subcommand reorder" -s h -l help -d 'Print help'
complete -c docket -n "__fish_docket_using_subcommand promote" -l to -d 'Jump to a kind instead of advancing one step' -r -f -a "handoff\t''
relay\t''
spec\t''"
complete -c docket -n "__fish_docket_using_subcommand promote" -l format -d 'Defaults to agent under Claude Code or off a terminal, human otherwise' -r -f -a "human\t''
agent\t''
json\t''"
complete -c docket -n "__fish_docket_using_subcommand promote" -l color -d 'Honours NO_COLOR and CLICOLOR_FORCE' -r -f -a "auto\t''
always\t''
never\t''"
complete -c docket -n "__fish_docket_using_subcommand promote" -l project -d 'Act on another project\'s docket' -r -F
complete -c docket -n "__fish_docket_using_subcommand promote" -l json -d 'Shorthand for --format json'
complete -c docket -n "__fish_docket_using_subcommand promote" -s q -l quiet -d 'Print only what was asked for'
complete -c docket -n "__fish_docket_using_subcommand promote" -s h -l help -d 'Print help'
complete -c docket -n "__fish_docket_using_subcommand relay" -l name -d 'Name of the successor, under the same rules as create' -r
complete -c docket -n "__fish_docket_using_subcommand relay" -l tagline -d 'Tagline of the successor. Use - for standard input' -r
complete -c docket -n "__fish_docket_using_subcommand relay" -l body -d 'Body of the successor. Use - for standard input' -r
complete -c docket -n "__fish_docket_using_subcommand relay" -l format -d 'Defaults to agent under Claude Code or off a terminal, human otherwise' -r -f -a "human\t''
agent\t''
json\t''"
complete -c docket -n "__fish_docket_using_subcommand relay" -l color -d 'Honours NO_COLOR and CLICOLOR_FORCE' -r -f -a "auto\t''
always\t''
never\t''"
complete -c docket -n "__fish_docket_using_subcommand relay" -l project -d 'Act on another project\'s docket' -r -F
complete -c docket -n "__fish_docket_using_subcommand relay" -l json -d 'Shorthand for --format json'
complete -c docket -n "__fish_docket_using_subcommand relay" -s q -l quiet -d 'Print only what was asked for'
complete -c docket -n "__fish_docket_using_subcommand relay" -s h -l help -d 'Print help'
complete -c docket -n "__fish_docket_using_subcommand close" -l format -d 'Defaults to agent under Claude Code or off a terminal, human otherwise' -r -f -a "human\t''
agent\t''
json\t''"
complete -c docket -n "__fish_docket_using_subcommand close" -l color -d 'Honours NO_COLOR and CLICOLOR_FORCE' -r -f -a "auto\t''
always\t''
never\t''"
complete -c docket -n "__fish_docket_using_subcommand close" -l project -d 'Act on another project\'s docket' -r -F
complete -c docket -n "__fish_docket_using_subcommand close" -l json -d 'Shorthand for --format json'
complete -c docket -n "__fish_docket_using_subcommand close" -s q -l quiet -d 'Print only what was asked for'
complete -c docket -n "__fish_docket_using_subcommand close" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c docket -n "__fish_docket_using_subcommand doctor" -l format -d 'Defaults to agent under Claude Code or off a terminal, human otherwise' -r -f -a "human\t''
agent\t''
json\t''"
complete -c docket -n "__fish_docket_using_subcommand doctor" -l color -d 'Honours NO_COLOR and CLICOLOR_FORCE' -r -f -a "auto\t''
always\t''
never\t''"
complete -c docket -n "__fish_docket_using_subcommand doctor" -l project -d 'Act on another project\'s docket' -r -F
complete -c docket -n "__fish_docket_using_subcommand doctor" -l json -d 'Shorthand for --format json'
complete -c docket -n "__fish_docket_using_subcommand doctor" -s q -l quiet -d 'Print only what was asked for'
complete -c docket -n "__fish_docket_using_subcommand doctor" -s h -l help -d 'Print help'
complete -c docket -n "__fish_docket_using_subcommand announce" -l format -d 'Defaults to agent under Claude Code or off a terminal, human otherwise' -r -f -a "human\t''
agent\t''
json\t''"
complete -c docket -n "__fish_docket_using_subcommand announce" -l color -d 'Honours NO_COLOR and CLICOLOR_FORCE' -r -f -a "auto\t''
always\t''
never\t''"
complete -c docket -n "__fish_docket_using_subcommand announce" -l project -d 'Act on another project\'s docket' -r -F
complete -c docket -n "__fish_docket_using_subcommand announce" -l hook -d 'Emit Claude Code SessionStart hook JSON'
complete -c docket -n "__fish_docket_using_subcommand announce" -l json -d 'Shorthand for --format json'
complete -c docket -n "__fish_docket_using_subcommand announce" -s q -l quiet -d 'Print only what was asked for'
complete -c docket -n "__fish_docket_using_subcommand announce" -s h -l help -d 'Print help'
complete -c docket -n "__fish_docket_using_subcommand help" -l format -d 'Defaults to agent under Claude Code or off a terminal, human otherwise' -r -f -a "human\t''
agent\t''
json\t''"
complete -c docket -n "__fish_docket_using_subcommand help" -l color -d 'Honours NO_COLOR and CLICOLOR_FORCE' -r -f -a "auto\t''
always\t''
never\t''"
complete -c docket -n "__fish_docket_using_subcommand help" -l project -d 'Act on another project\'s docket' -r -F
complete -c docket -n "__fish_docket_using_subcommand help" -l json -d 'Shorthand for --format json'
complete -c docket -n "__fish_docket_using_subcommand help" -s q -l quiet -d 'Print only what was asked for'
complete -c docket -n "__fish_docket_using_subcommand help" -s h -l help -d 'Print help'
complete -c docket -n "__fish_docket_using_subcommand guide" -l format -d 'Defaults to agent under Claude Code or off a terminal, human otherwise' -r -f -a "human\t''
agent\t''
json\t''"
complete -c docket -n "__fish_docket_using_subcommand guide" -l color -d 'Honours NO_COLOR and CLICOLOR_FORCE' -r -f -a "auto\t''
always\t''
never\t''"
complete -c docket -n "__fish_docket_using_subcommand guide" -l project -d 'Act on another project\'s docket' -r -F
complete -c docket -n "__fish_docket_using_subcommand guide" -l json -d 'Shorthand for --format json'
complete -c docket -n "__fish_docket_using_subcommand guide" -s q -l quiet -d 'Print only what was asked for'
complete -c docket -n "__fish_docket_using_subcommand guide" -s h -l help -d 'Print help'
complete -c docket -n "__fish_docket_using_subcommand completions" -l format -d 'Defaults to agent under Claude Code or off a terminal, human otherwise' -r -f -a "human\t''
agent\t''
json\t''"
complete -c docket -n "__fish_docket_using_subcommand completions" -l color -d 'Honours NO_COLOR and CLICOLOR_FORCE' -r -f -a "auto\t''
always\t''
never\t''"
complete -c docket -n "__fish_docket_using_subcommand completions" -l project -d 'Act on another project\'s docket' -r -F
complete -c docket -n "__fish_docket_using_subcommand completions" -l json -d 'Shorthand for --format json'
complete -c docket -n "__fish_docket_using_subcommand completions" -s q -l quiet -d 'Print only what was asked for'
complete -c docket -n "__fish_docket_using_subcommand completions" -s h -l help -d 'Print help (see more with \'--help\')'
