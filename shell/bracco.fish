# Bracco fuzzy completion for fish. Source this file from ~/.config/fish/config.fish.
# Type `nvim **` and press Tab to pick a path with bracco.
function __bracco_fuzzy_complete
    set -l before (commandline --current-process --cut-at-cursor)
    if string match -q '*\*\*' -- $before
        set -l selected (command bracco --absolute)
        if test $status -eq 0; and test -n "$selected"
            set -l prefix (string replace -r '\*\*$' '' -- $before)
            set -l quoted (string escape --style=script -- "$selected")
            commandline --current-process "$prefix$quoted "
        end
    else
        commandline -f complete
    end
end

bind \t __bracco_fuzzy_complete
