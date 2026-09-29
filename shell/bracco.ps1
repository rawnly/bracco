# Bracco fuzzy completion for PowerShell. Add this to your $PROFILE.
# Type `nvim **` and press Tab to pick a path with bracco.
Set-PSReadLineKeyHandler -Key Tab -ScriptBlock {
    $line = $null
    $cursor = 0
    [Microsoft.PowerShell.PSConsoleReadLine]::GetBufferState([ref]$line, [ref]$cursor)
    $before = $line.Substring(0, $cursor)

    if ($before.EndsWith('**', [System.StringComparison]::Ordinal)) {
        $selected = & bracco --absolute
        if ($LASTEXITCODE -eq 0 -and $selected) {
            $prefix = $before.Substring(0, $before.Length - 2)
            $quoted = "'" + ([string]$selected).Replace("'", "''") + "' "
            [Microsoft.PowerShell.PSConsoleReadLine]::Replace(0, $line.Length, ($prefix + $quoted))
        }
    } else {
        [Microsoft.PowerShell.PSConsoleReadLine]::TabCompleteNext()
    }
}
