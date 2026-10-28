-- Rex plugin for bracco: pick a file in a floating pane, open it in a new tab.
-- Registers the `bracco_pick` action; bind it yourself or pass `key`.
local M = {}

local TERM = "com.superlogical.terminal"
local SHELL = TERM .. ".shell"

-- Runs inside the float. $1 bracco, $2 editor (may be empty), $3 client id (may be empty), rest are bracco args.
-- UI is drawn on /dev/tty, so the pick comes back on stdout.
local SCRIPT = [[
bin=$1; editor=${2:-${BRACCO_EDITOR:-${VISUAL:-${EDITOR:-vi}}}}; client=$3; shift 3
f=$("$bin" --absolute "$@") || exit 0
[ -n "$f" ] || exit 0
if command -v jq >/dev/null; then
  win=$(rex window new -c "$PWD" --focus=false --json "$(basename "$f")" -- sh -c 'exec $0 "$1"' "$editor" "$f" | jq -r .window_id) || exit 1
  # `rex window focus` moves the server's active window only; the app client
  # needs session.select to switch tabs. With several clients connected the
  # target is ambiguous, so address the one that ran the action, falling back
  # to the default when that is not an app client (e.g. `rex do` from a shell).
  { [ -n "$client" ] && rex -C "$client" do session.select session_id="$REX_SESSION" window_id="$win" 2>/dev/null; } ||
    rex do session.select session_id="$REX_SESSION" window_id="$win"
else
  rex window new -c "$PWD" "$(basename "$f")" -- sh -c 'exec $0 "$1"' "$editor" "$f" >/dev/null || exit 1
fi
exec rex block close "$REX_BLOCK"
]]

--- Working directory of a block: the reported `file://` URI, else the foreground process cwd, else $HOME.
local function cwd_of(block_id)
	local r = rex.block.call(TERM, "pwd", { block_id = block_id })
	local path = r and r.pwd and r.pwd:match("^file://[^/]*(/.*)$")
	if path then
		return (path:gsub("%%(%x%x)", function(hex)
			return string.char(tonumber(hex, 16))
		end))
	end
	local proc = rex.block.call(TERM, "process", { block_id = block_id })
	return (proc and proc.foreground and proc.foreground.cwd) or os.getenv("HOME")
end

---@class BraccoRexOptions
---@field bin? string        bracco executable (default "bracco")
---@field editor? string     editor command; default $BRACCO_EDITOR, $VISUAL, $EDITOR, else "vi"
---@field args? string[]     extra bracco flags, e.g. { "--vim", "--preview", "bat {}" }
---@field bounds? number[]   float geometry `{ x, y, w, h }` as window fractions (default centered 60% x 70%)
---@field key? string        optional key binding for the action, e.g. "ctrl+a>i"

---@param opts? BraccoRexOptions
function M.setup(opts)
	opts = opts or {}

	rex.action({
		name = "bracco_pick",
		title = "bracco: open file",
		run = function(ctx)
			-- Empty editor: the script falls back to the float's own $BRACCO_EDITOR/$VISUAL/$EDITOR.
			local cmd = { "sh", "-c", SCRIPT, "bracco", opts.bin or "bracco", opts.editor or "", ctx.client_id or "" }
			for _, arg in ipairs(opts.args or {}) do
				cmd[#cmd + 1] = arg
			end

			local result, err = rex.session.new_layer({
				bounds = opts.bounds or { 0.2, 0.15, 0.6, 0.7 },
				focus = true,
				layout = rex.layout.block({
					flavor = SHELL,
					options = { cwd = cwd_of(ctx.block_id), command = cmd },
				}),
			})
			if err then
				error(err, 0)
			end
			return result
		end,
	})

	if opts.key then
		rex.bind(opts.key, "bracco_pick")
	end
end

return M
