#!/usr/bin/env bash
# Herdr plugin entrypoint for fff-picker.
#
#   launch  action: open the picker popup in the caller's cwd
#   picker  popup:  run fff-picker, then open the editor overlay on the result
#   edit    overlay: exec $FFF_PICKER_EDITOR / $VISUAL / $EDITOR on $FFF_PICKER_FILE
#
# Env knobs: FFF_PICKER_BIN (default fff-picker), FFF_PICKER_EDITOR.
set -euo pipefail

HERDR="${HERDR_BIN_PATH:-herdr}"
PLUGIN="${HERDR_PLUGIN_ID:-dev.rawnly.fff-picker}"
PICKER="${FFF_PICKER_BIN:-fff-picker}"

die() { printf 'fff-picker plugin: %s\n' "$*" >&2; exit 1; }

context_cwd() {
  local cwd=""
  if [[ -n "${HERDR_PLUGIN_CONTEXT_JSON:-}" ]] && command -v jq >/dev/null; then
    cwd=$(jq -r '.workspace_cwd // .focused_pane_cwd // empty' <<<"$HERDR_PLUGIN_CONTEXT_JSON")
  fi
  printf '%s' "${cwd:-${HERDR_ACTIVE_PANE_CWD:-$PWD}}"
}

case "${1:-}" in
  launch)
    "$HERDR" plugin pane open --plugin "$PLUGIN" --entrypoint picker \
      --placement popup --cwd "$(context_cwd)" --focus
    ;;

  picker)
    command -v "$PICKER" >/dev/null || die "'$PICKER' not found in PATH"
    # UI is drawn on /dev/tty; selection comes back on stdout.
    file=$("$PICKER" --absolute) || exit 0   # cancelled / no match: just close
    [[ -n "$file" ]] || exit 0
    "$HERDR" plugin pane open --plugin "$PLUGIN" --entrypoint editor \
      --placement overlay --cwd "$PWD" --env "FFF_PICKER_FILE=$file" --focus
    ;;

  edit)
    [[ -n "${FFF_PICKER_FILE:-}" ]] || die "FFF_PICKER_FILE is not set"
    editor="${FFF_PICKER_EDITOR:-${VISUAL:-${EDITOR:-vi}}}"
    # $editor may carry args (e.g. "code -w"), so let the shell split it.
    # shellcheck disable=SC2086
    exec $editor "$FFF_PICKER_FILE"
    ;;

  *) die "usage: $0 launch|picker|edit" ;;
esac
