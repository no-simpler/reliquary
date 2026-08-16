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

complete -c docket -n "__fish_docket_needs_command" -l format -d 'Output shape. Defaults to human at a terminal and agent everywhere else, including under Claude Code' -r -f -a "human\t'Aligned, coloured table. The default when a person is watching'
agent\t'Unaligned, uncoloured lines with a stable field order'
json\t'Machine-readable, for scripting'"
complete -c docket -n "__fish_docket_needs_command" -l color -d 'When to colour. Honours NO_COLOR and CLICOLOR_FORCE' -r -f -a "auto\t''
always\t''
never\t''"
complete -c docket -n "__fish_docket_needs_command" -l project -d 'Act on this project\'s docket instead of the one for the working directory' -r -F
complete -c docket -n "__fish_docket_needs_command" -l json -d 'Shorthand for --format json'
complete -c docket -n "__fish_docket_needs_command" -s q -l quiet -d 'Print only what was asked for, with no confirmations'
complete -c docket -n "__fish_docket_needs_command" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c docket -n "__fish_docket_needs_command" -s V -l version -d 'Print version'
complete -c docket -n "__fish_docket_needs_command" -f -a "list" -d 'List outstanding items'
complete -c docket -n "__fish_docket_needs_command" -f -a "ls" -d 'List outstanding items'
complete -c docket -n "__fish_docket_needs_command" -f -a "create" -d 'Open a new item and print where to write its body'
complete -c docket -n "__fish_docket_needs_command" -f -a "show" -d 'Print an item\'s body'
complete -c docket -n "__fish_docket_needs_command" -f -a "path" -d 'Print an item\'s file path, for writing or editing its body'
complete -c docket -n "__fish_docket_needs_command" -f -a "set" -d 'Change an item\'s descriptive metadata'
complete -c docket -n "__fish_docket_needs_command" -f -a "reorder" -d 'Change where an item sits in the order'
complete -c docket -n "__fish_docket_needs_command" -f -a "promote" -d 'Advance an item one rung along the ladder'
complete -c docket -n "__fish_docket_needs_command" -f -a "relay" -d 'Consume a relay: open its successor and archive it'
complete -c docket -n "__fish_docket_needs_command" -f -a "close" -d 'Archive an item whose work is done'
complete -c docket -n "__fish_docket_needs_command" -f -a "delete" -d 'Remove an item outright, leaving no archive copy'
complete -c docket -n "__fish_docket_needs_command" -f -a "doctor" -d 'Check the depot for damage'
complete -c docket -n "__fish_docket_needs_command" -f -a "announce" -d 'Emit the session-start announcement'
complete -c docket -n "__fish_docket_needs_command" -f -a "help" -d 'Explain a topic, or a command'
complete -c docket -n "__fish_docket_needs_command" -f -a "completions" -d 'Print a shell completion script'
complete -c docket -n "__fish_docket_using_subcommand list" -l kind -d 'Only this kind' -r -f -a "handoff\t''
relay\t''
spec\t''"
complete -c docket -n "__fish_docket_using_subcommand list" -l format -d 'Output shape. Defaults to human at a terminal and agent everywhere else, including under Claude Code' -r -f -a "human\t'Aligned, coloured table. The default when a person is watching'
agent\t'Unaligned, uncoloured lines with a stable field order'
json\t'Machine-readable, for scripting'"
complete -c docket -n "__fish_docket_using_subcommand list" -l color -d 'When to colour. Honours NO_COLOR and CLICOLOR_FORCE' -r -f -a "auto\t''
always\t''
never\t''"
complete -c docket -n "__fish_docket_using_subcommand list" -l project -d 'Act on this project\'s docket instead of the one for the working directory' -r -F
complete -c docket -n "__fish_docket_using_subcommand list" -l all -d 'Every project on this machine, not just this one'
complete -c docket -n "__fish_docket_using_subcommand list" -l blocked -d 'Only items carrying a block'
complete -c docket -n "__fish_docket_using_subcommand list" -l invalid -d 'Only items whose frontmatter will not parse'
complete -c docket -n "__fish_docket_using_subcommand list" -l archived -d 'What has been closed, instead of what is open'
complete -c docket -n "__fish_docket_using_subcommand list" -l json -d 'Shorthand for --format json'
complete -c docket -n "__fish_docket_using_subcommand list" -s q -l quiet -d 'Print only what was asked for, with no confirmations'
complete -c docket -n "__fish_docket_using_subcommand list" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c docket -n "__fish_docket_using_subcommand ls" -l kind -d 'Only this kind' -r -f -a "handoff\t''
relay\t''
spec\t''"
complete -c docket -n "__fish_docket_using_subcommand ls" -l format -d 'Output shape. Defaults to human at a terminal and agent everywhere else, including under Claude Code' -r -f -a "human\t'Aligned, coloured table. The default when a person is watching'
agent\t'Unaligned, uncoloured lines with a stable field order'
json\t'Machine-readable, for scripting'"
complete -c docket -n "__fish_docket_using_subcommand ls" -l color -d 'When to colour. Honours NO_COLOR and CLICOLOR_FORCE' -r -f -a "auto\t''
always\t''
never\t''"
complete -c docket -n "__fish_docket_using_subcommand ls" -l project -d 'Act on this project\'s docket instead of the one for the working directory' -r -F
complete -c docket -n "__fish_docket_using_subcommand ls" -l all -d 'Every project on this machine, not just this one'
complete -c docket -n "__fish_docket_using_subcommand ls" -l blocked -d 'Only items carrying a block'
complete -c docket -n "__fish_docket_using_subcommand ls" -l invalid -d 'Only items whose frontmatter will not parse'
complete -c docket -n "__fish_docket_using_subcommand ls" -l archived -d 'What has been closed, instead of what is open'
complete -c docket -n "__fish_docket_using_subcommand ls" -l json -d 'Shorthand for --format json'
complete -c docket -n "__fish_docket_using_subcommand ls" -s q -l quiet -d 'Print only what was asked for, with no confirmations'
complete -c docket -n "__fish_docket_using_subcommand ls" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c docket -n "__fish_docket_using_subcommand create" -l title -d 'The item\'s name, at most 72 characters. `-` reads standard input' -r
complete -c docket -n "__fish_docket_using_subcommand create" -l tagline -d 'One line under the title, at most 80 characters. `-` reads standard input' -r
complete -c docket -n "__fish_docket_using_subcommand create" -l to -d 'Open it for another project. Defaults to this one' -r -F
complete -c docket -n "__fish_docket_using_subcommand create" -l body -d 'Body to write, instead of leaving the file for you to fill in. `-` reads standard input' -r
complete -c docket -n "__fish_docket_using_subcommand create" -l format -d 'Output shape. Defaults to human at a terminal and agent everywhere else, including under Claude Code' -r -f -a "human\t'Aligned, coloured table. The default when a person is watching'
agent\t'Unaligned, uncoloured lines with a stable field order'
json\t'Machine-readable, for scripting'"
complete -c docket -n "__fish_docket_using_subcommand create" -l color -d 'When to colour. Honours NO_COLOR and CLICOLOR_FORCE' -r -f -a "auto\t''
always\t''
never\t''"
complete -c docket -n "__fish_docket_using_subcommand create" -l project -d 'Act on this project\'s docket instead of the one for the working directory' -r -F
complete -c docket -n "__fish_docket_using_subcommand create" -l allow-missing -d 'Allow a target directory that does not exist yet'
complete -c docket -n "__fish_docket_using_subcommand create" -l json -d 'Shorthand for --format json'
complete -c docket -n "__fish_docket_using_subcommand create" -s q -l quiet -d 'Print only what was asked for, with no confirmations'
complete -c docket -n "__fish_docket_using_subcommand create" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c docket -n "__fish_docket_using_subcommand show" -l format -d 'Output shape. Defaults to human at a terminal and agent everywhere else, including under Claude Code' -r -f -a "human\t'Aligned, coloured table. The default when a person is watching'
agent\t'Unaligned, uncoloured lines with a stable field order'
json\t'Machine-readable, for scripting'"
complete -c docket -n "__fish_docket_using_subcommand show" -l color -d 'When to colour. Honours NO_COLOR and CLICOLOR_FORCE' -r -f -a "auto\t''
always\t''
never\t''"
complete -c docket -n "__fish_docket_using_subcommand show" -l project -d 'Act on this project\'s docket instead of the one for the working directory' -r -F
complete -c docket -n "__fish_docket_using_subcommand show" -l json -d 'Shorthand for --format json'
complete -c docket -n "__fish_docket_using_subcommand show" -s q -l quiet -d 'Print only what was asked for, with no confirmations'
complete -c docket -n "__fish_docket_using_subcommand show" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c docket -n "__fish_docket_using_subcommand path" -l format -d 'Output shape. Defaults to human at a terminal and agent everywhere else, including under Claude Code' -r -f -a "human\t'Aligned, coloured table. The default when a person is watching'
agent\t'Unaligned, uncoloured lines with a stable field order'
json\t'Machine-readable, for scripting'"
complete -c docket -n "__fish_docket_using_subcommand path" -l color -d 'When to colour. Honours NO_COLOR and CLICOLOR_FORCE' -r -f -a "auto\t''
always\t''
never\t''"
complete -c docket -n "__fish_docket_using_subcommand path" -l project -d 'Act on this project\'s docket instead of the one for the working directory' -r -F
complete -c docket -n "__fish_docket_using_subcommand path" -l json -d 'Shorthand for --format json'
complete -c docket -n "__fish_docket_using_subcommand path" -s q -l quiet -d 'Print only what was asked for, with no confirmations'
complete -c docket -n "__fish_docket_using_subcommand path" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c docket -n "__fish_docket_using_subcommand set" -l title -d 'Replace the title. `-` reads standard input' -r
complete -c docket -n "__fish_docket_using_subcommand set" -l tagline -d 'Replace the tagline. `-` reads standard input' -r
complete -c docket -n "__fish_docket_using_subcommand set" -l blocked -d 'Record what must clear before this item can move, in one line. `-` reads standard input' -r
complete -c docket -n "__fish_docket_using_subcommand set" -l tags -d 'Replace the tags wholesale' -r
complete -c docket -n "__fish_docket_using_subcommand set" -l format -d 'Output shape. Defaults to human at a terminal and agent everywhere else, including under Claude Code' -r -f -a "human\t'Aligned, coloured table. The default when a person is watching'
agent\t'Unaligned, uncoloured lines with a stable field order'
json\t'Machine-readable, for scripting'"
complete -c docket -n "__fish_docket_using_subcommand set" -l color -d 'When to colour. Honours NO_COLOR and CLICOLOR_FORCE' -r -f -a "auto\t''
always\t''
never\t''"
complete -c docket -n "__fish_docket_using_subcommand set" -l project -d 'Act on this project\'s docket instead of the one for the working directory' -r -F
complete -c docket -n "__fish_docket_using_subcommand set" -l clear-blocked -d 'Drop the block'
complete -c docket -n "__fish_docket_using_subcommand set" -l json -d 'Shorthand for --format json'
complete -c docket -n "__fish_docket_using_subcommand set" -s q -l quiet -d 'Print only what was asked for, with no confirmations'
complete -c docket -n "__fish_docket_using_subcommand set" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c docket -n "__fish_docket_using_subcommand reorder" -l position -d 'Move it to this position, counting from one' -r
complete -c docket -n "__fish_docket_using_subcommand reorder" -l before -d 'Move it directly ahead of this item' -r
complete -c docket -n "__fish_docket_using_subcommand reorder" -l after -d 'Move it directly behind this item' -r
complete -c docket -n "__fish_docket_using_subcommand reorder" -l sequence -d 'Reorder in bulk. Listed items move to the front in this order' -r
complete -c docket -n "__fish_docket_using_subcommand reorder" -l format -d 'Output shape. Defaults to human at a terminal and agent everywhere else, including under Claude Code' -r -f -a "human\t'Aligned, coloured table. The default when a person is watching'
agent\t'Unaligned, uncoloured lines with a stable field order'
json\t'Machine-readable, for scripting'"
complete -c docket -n "__fish_docket_using_subcommand reorder" -l color -d 'When to colour. Honours NO_COLOR and CLICOLOR_FORCE' -r -f -a "auto\t''
always\t''
never\t''"
complete -c docket -n "__fish_docket_using_subcommand reorder" -l project -d 'Act on this project\'s docket instead of the one for the working directory' -r -F
complete -c docket -n "__fish_docket_using_subcommand reorder" -l top -d 'Move it to the front'
complete -c docket -n "__fish_docket_using_subcommand reorder" -l bottom -d 'Move it to the back'
complete -c docket -n "__fish_docket_using_subcommand reorder" -l json -d 'Shorthand for --format json'
complete -c docket -n "__fish_docket_using_subcommand reorder" -s q -l quiet -d 'Print only what was asked for, with no confirmations'
complete -c docket -n "__fish_docket_using_subcommand reorder" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c docket -n "__fish_docket_using_subcommand promote" -l to -d 'Jump to a rung instead of advancing one step' -r -f -a "handoff\t''
relay\t''
spec\t''"
complete -c docket -n "__fish_docket_using_subcommand promote" -l format -d 'Output shape. Defaults to human at a terminal and agent everywhere else, including under Claude Code' -r -f -a "human\t'Aligned, coloured table. The default when a person is watching'
agent\t'Unaligned, uncoloured lines with a stable field order'
json\t'Machine-readable, for scripting'"
complete -c docket -n "__fish_docket_using_subcommand promote" -l color -d 'When to colour. Honours NO_COLOR and CLICOLOR_FORCE' -r -f -a "auto\t''
always\t''
never\t''"
complete -c docket -n "__fish_docket_using_subcommand promote" -l project -d 'Act on this project\'s docket instead of the one for the working directory' -r -F
complete -c docket -n "__fish_docket_using_subcommand promote" -l json -d 'Shorthand for --format json'
complete -c docket -n "__fish_docket_using_subcommand promote" -s q -l quiet -d 'Print only what was asked for, with no confirmations'
complete -c docket -n "__fish_docket_using_subcommand promote" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c docket -n "__fish_docket_using_subcommand relay" -l title -d 'Title of the successor. `-` reads standard input' -r
complete -c docket -n "__fish_docket_using_subcommand relay" -l tagline -d 'Tagline of the successor. `-` reads standard input' -r
complete -c docket -n "__fish_docket_using_subcommand relay" -l body -d 'Body of the successor. `-` reads standard input' -r
complete -c docket -n "__fish_docket_using_subcommand relay" -l format -d 'Output shape. Defaults to human at a terminal and agent everywhere else, including under Claude Code' -r -f -a "human\t'Aligned, coloured table. The default when a person is watching'
agent\t'Unaligned, uncoloured lines with a stable field order'
json\t'Machine-readable, for scripting'"
complete -c docket -n "__fish_docket_using_subcommand relay" -l color -d 'When to colour. Honours NO_COLOR and CLICOLOR_FORCE' -r -f -a "auto\t''
always\t''
never\t''"
complete -c docket -n "__fish_docket_using_subcommand relay" -l project -d 'Act on this project\'s docket instead of the one for the working directory' -r -F
complete -c docket -n "__fish_docket_using_subcommand relay" -l json -d 'Shorthand for --format json'
complete -c docket -n "__fish_docket_using_subcommand relay" -s q -l quiet -d 'Print only what was asked for, with no confirmations'
complete -c docket -n "__fish_docket_using_subcommand relay" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c docket -n "__fish_docket_using_subcommand close" -l format -d 'Output shape. Defaults to human at a terminal and agent everywhere else, including under Claude Code' -r -f -a "human\t'Aligned, coloured table. The default when a person is watching'
agent\t'Unaligned, uncoloured lines with a stable field order'
json\t'Machine-readable, for scripting'"
complete -c docket -n "__fish_docket_using_subcommand close" -l color -d 'When to colour. Honours NO_COLOR and CLICOLOR_FORCE' -r -f -a "auto\t''
always\t''
never\t''"
complete -c docket -n "__fish_docket_using_subcommand close" -l project -d 'Act on this project\'s docket instead of the one for the working directory' -r -F
complete -c docket -n "__fish_docket_using_subcommand close" -l json -d 'Shorthand for --format json'
complete -c docket -n "__fish_docket_using_subcommand close" -s q -l quiet -d 'Print only what was asked for, with no confirmations'
complete -c docket -n "__fish_docket_using_subcommand close" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c docket -n "__fish_docket_using_subcommand delete" -l format -d 'Output shape. Defaults to human at a terminal and agent everywhere else, including under Claude Code' -r -f -a "human\t'Aligned, coloured table. The default when a person is watching'
agent\t'Unaligned, uncoloured lines with a stable field order'
json\t'Machine-readable, for scripting'"
complete -c docket -n "__fish_docket_using_subcommand delete" -l color -d 'When to colour. Honours NO_COLOR and CLICOLOR_FORCE' -r -f -a "auto\t''
always\t''
never\t''"
complete -c docket -n "__fish_docket_using_subcommand delete" -l project -d 'Act on this project\'s docket instead of the one for the working directory' -r -F
complete -c docket -n "__fish_docket_using_subcommand delete" -s f -l force -d 'Skip the confirmation'
complete -c docket -n "__fish_docket_using_subcommand delete" -l json -d 'Shorthand for --format json'
complete -c docket -n "__fish_docket_using_subcommand delete" -s q -l quiet -d 'Print only what was asked for, with no confirmations'
complete -c docket -n "__fish_docket_using_subcommand delete" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c docket -n "__fish_docket_using_subcommand doctor" -l format -d 'Output shape. Defaults to human at a terminal and agent everywhere else, including under Claude Code' -r -f -a "human\t'Aligned, coloured table. The default when a person is watching'
agent\t'Unaligned, uncoloured lines with a stable field order'
json\t'Machine-readable, for scripting'"
complete -c docket -n "__fish_docket_using_subcommand doctor" -l color -d 'When to colour. Honours NO_COLOR and CLICOLOR_FORCE' -r -f -a "auto\t''
always\t''
never\t''"
complete -c docket -n "__fish_docket_using_subcommand doctor" -l project -d 'Act on this project\'s docket instead of the one for the working directory' -r -F
complete -c docket -n "__fish_docket_using_subcommand doctor" -l json -d 'Shorthand for --format json'
complete -c docket -n "__fish_docket_using_subcommand doctor" -s q -l quiet -d 'Print only what was asked for, with no confirmations'
complete -c docket -n "__fish_docket_using_subcommand doctor" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c docket -n "__fish_docket_using_subcommand announce" -l format -d 'Output shape. Defaults to human at a terminal and agent everywhere else, including under Claude Code' -r -f -a "human\t'Aligned, coloured table. The default when a person is watching'
agent\t'Unaligned, uncoloured lines with a stable field order'
json\t'Machine-readable, for scripting'"
complete -c docket -n "__fish_docket_using_subcommand announce" -l color -d 'When to colour. Honours NO_COLOR and CLICOLOR_FORCE' -r -f -a "auto\t''
always\t''
never\t''"
complete -c docket -n "__fish_docket_using_subcommand announce" -l project -d 'Act on this project\'s docket instead of the one for the working directory' -r -F
complete -c docket -n "__fish_docket_using_subcommand announce" -l hook -d 'Emit Claude Code SessionStart hook JSON'
complete -c docket -n "__fish_docket_using_subcommand announce" -l json -d 'Shorthand for --format json'
complete -c docket -n "__fish_docket_using_subcommand announce" -s q -l quiet -d 'Print only what was asked for, with no confirmations'
complete -c docket -n "__fish_docket_using_subcommand announce" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c docket -n "__fish_docket_using_subcommand help" -l format -d 'Output shape. Defaults to human at a terminal and agent everywhere else, including under Claude Code' -r -f -a "human\t'Aligned, coloured table. The default when a person is watching'
agent\t'Unaligned, uncoloured lines with a stable field order'
json\t'Machine-readable, for scripting'"
complete -c docket -n "__fish_docket_using_subcommand help" -l color -d 'When to colour. Honours NO_COLOR and CLICOLOR_FORCE' -r -f -a "auto\t''
always\t''
never\t''"
complete -c docket -n "__fish_docket_using_subcommand help" -l project -d 'Act on this project\'s docket instead of the one for the working directory' -r -F
complete -c docket -n "__fish_docket_using_subcommand help" -l json -d 'Shorthand for --format json'
complete -c docket -n "__fish_docket_using_subcommand help" -s q -l quiet -d 'Print only what was asked for, with no confirmations'
complete -c docket -n "__fish_docket_using_subcommand help" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c docket -n "__fish_docket_using_subcommand completions" -l format -d 'Output shape. Defaults to human at a terminal and agent everywhere else, including under Claude Code' -r -f -a "human\t'Aligned, coloured table. The default when a person is watching'
agent\t'Unaligned, uncoloured lines with a stable field order'
json\t'Machine-readable, for scripting'"
complete -c docket -n "__fish_docket_using_subcommand completions" -l color -d 'When to colour. Honours NO_COLOR and CLICOLOR_FORCE' -r -f -a "auto\t''
always\t''
never\t''"
complete -c docket -n "__fish_docket_using_subcommand completions" -l project -d 'Act on this project\'s docket instead of the one for the working directory' -r -F
complete -c docket -n "__fish_docket_using_subcommand completions" -l json -d 'Shorthand for --format json'
complete -c docket -n "__fish_docket_using_subcommand completions" -s q -l quiet -d 'Print only what was asked for, with no confirmations'
complete -c docket -n "__fish_docket_using_subcommand completions" -s h -l help -d 'Print help (see more with \'--help\')'
