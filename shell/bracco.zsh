# Bracco fuzzy completion for zsh. Source this file from ~/.zshrc.
# Type `nvim **` and press Tab to pick a path with bracco.
_bracco_fuzzy_complete() {
  if [[ "$LBUFFER" == *'**' ]]; then
    local selected
    selected=$(command bracco --absolute) || return
    if [[ -n "$selected" ]]; then
      LBUFFER="${LBUFFER%\*\*}${(q)selected} "
      RBUFFER=""
    fi
  else
    zle expand-or-complete
  fi
}

zle -N bracco-fuzzy-complete _bracco_fuzzy_complete
bindkey '^I' bracco-fuzzy-complete
