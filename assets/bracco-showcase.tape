# Run from the repository root: vhs assets/bracco-showcase.tape
# A throwaway clone keeps the demo's git statuses predictable without touching the worktree.
Output assets/bracco-showcase.gif

Require bracco
Require bat
Require git

Set Shell bash
Set FontFamily "JetBrainsMono Nerd Font Mono"
Set FontSize 16
Set Width 1100
Set Height 610
Set Padding 18
Set BorderRadius 10
Set Theme "TokyoNight"
Set TypingSpeed 55ms

Hide
Type "demo=$(mktemp -d); git clone -q . $demo; cd $demo; printf '\n// Showcase change\n' >> src/main.rs; printf '\n# Showcase change\n' >> README.md; git add README.md; printf 'A new idea\n' > NOTES.md; export PS1='bracco ❯ '; clear"
Enter
Sleep 1s
Show

# Modified, staged, and untracked files rise to the top. Preview real source code.
Type "bracco --preview 'bat --color=always --style=plain {}'"
Enter
Sleep 2s

# Narrow to Rust sources, then show the changed-only git filter.
Type "*.rs"
Sleep 1s
Ctrl+g
Sleep 2s

# Clear the query to show every kind of changed file.
Ctrl+x
Sleep 1s
Down
Sleep 1s
Down
Sleep 1s

# Restore all files and search by directory, then return a path to the shell.
Ctrl+g
Type "/src/"
Sleep 2s
Enter
Sleep 2s

Hide
Type "cd - >/dev/null; rm -rf $demo"
Enter
