# Bracco fuzzy completion for bash. Source this file from ~/.bashrc.
# Type `nvim **` and press Tab to pick a path with bracco.
__bracco_fuzzy_complete() {
  local before="${READLINE_LINE:0:READLINE_POINT}"
  if [[ "$before" == *'**' ]]; then
    local selected
    selected=$(command bracco --absolute) || return
    if [[ -n "$selected" ]]; then
      local quoted
      printf -v quoted '%q' "$selected"
      READLINE_LINE="${before%\*\*}${quoted}${READLINE_LINE:READLINE_POINT}"
      READLINE_POINT=$((${#before} - 2 + ${#quoted}))
    fi
    return
  fi

  # Readline bind-x widgets replace Tab's native completer. Keep useful basic
  # file completion for ordinary path words, without changing the input on a miss.
  local word="${before##*[[:space:]]}" candidate quoted
  local -a matches=()
  while IFS= read -r candidate; do matches+=("$candidate"); done \
    < <(compgen -f -- "$word")
  if ((${#matches[@]} == 1)); then
    printf -v quoted '%q' "${matches[0]}"
    READLINE_LINE="${before:0:${#before}-${#word}}${quoted}${READLINE_LINE:READLINE_POINT}"
    READLINE_POINT=$((${#before} - ${#word} + ${#quoted}))
  elif ((${#matches[@]} > 1)); then
    printf '\n%s\n' "${matches[@]}" >&2
    READLINE_LINE="$before${READLINE_LINE:READLINE_POINT}"
    READLINE_POINT=${#before}
  fi
}

bind -x '"\C-i": __bracco_fuzzy_complete'
