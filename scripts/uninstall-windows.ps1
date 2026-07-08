<#
  Remove Campfire's saved data for a clean uninstall.

  Campfire keeps only two files: your server list (servers.toml) and its
  running-state (running.json). On Windows the `directories` crate splits them
  across the Roaming and Local app-data trees, so this removes both folders.
  It does NOT remove the app itself.

  Usage:
    .\scripts\uninstall-windows.ps1          # shows the paths and asks first
    .\scripts\uninstall-windows.ps1 -Yes     # delete without the prompt
#>

[CmdletBinding()]
param([switch]$Yes)

$ErrorActionPreference = 'Stop'

# Guard against empty env vars: Join-Path '' 'heonny\campfire' yields a bare
# relative path that would resolve against the current directory, so Remove-Item
# could delete an unrelated folder. Mirrors the macOS script's ${HOME:?} guard.
if (-not $env:APPDATA -or -not $env:LOCALAPPDATA) {
    Write-Error 'APPDATA or LOCALAPPDATA is not set - cannot locate Campfire data.'
    exit 1
}

# config_dir -> %APPDATA%\heonny\campfire\config
# data_local_dir -> %LOCALAPPDATA%\heonny\campfire\data
# Remove the shared `heonny\campfire` parent in each tree, leaving `heonny` be.
$targets = @(
    (Join-Path $env:APPDATA      'heonny\campfire'),
    (Join-Path $env:LOCALAPPDATA 'heonny\campfire')
)

$existing = @($targets | Where-Object { Test-Path -LiteralPath $_ })

if ($existing.Count -eq 0) {
    Write-Host 'Nothing to remove. Campfire has no saved data.'
    exit 0
}

Write-Host "This permanently deletes Campfire's saved data (server list + state):"
$existing | ForEach-Object { Write-Host "  $_" }
Write-Host ''
Write-Host 'The app itself is left alone.'
Write-Host ''

if (-not $Yes) {
    $reply = Read-Host 'Delete it? [y/N]'
    if ($reply -notmatch '^(y|yes)$') {
        Write-Host 'Aborted.'
        exit 0
    }
}

foreach ($path in $existing) {
    Remove-Item -Recurse -Force -LiteralPath $path
    Write-Host "==> removed $path"
}
