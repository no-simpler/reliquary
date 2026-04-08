##
## Prompt: oh-my-posh
##

if command -q oh-my-posh; and test "$TERM_PROGRAM" != Apple_Terminal
    oh-my-posh init fish --config ~/.config/oh-my-posh/dreamsofautonomy.toml | source
else
    ## Fallback: minimal prompt with color-coded exit status
    function fish_prompt
        set -l last_status $status
        if test $last_status -eq 0
            set_color green
        else
            set_color red
        end
        echo -n (prompt_pwd)' % '
        set_color normal
    end
end
