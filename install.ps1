# Installs MDeer for the current user (no admin rights needed):
#   irm https://raw.githubusercontent.com/philippschmalen/mdeer/main/install.ps1 | iex
#
# PowerShell downloads carry no "downloaded from the internet" mark, so SmartScreen
# doesn't block the unsigned exe the way it does for browser downloads.
$ErrorActionPreference = 'Stop'

$dir = Join-Path $env:LOCALAPPDATA 'Programs\MDeer'
$exe = Join-Path $dir 'mdeer.exe'
New-Item -ItemType Directory -Force $dir | Out-Null
Invoke-WebRequest 'https://github.com/philippschmalen/mdeer/releases/latest/download/mdeer.exe' -OutFile $exe
Unblock-File $exe

$path = [Environment]::GetEnvironmentVariable('Path', 'User')
if (($path -split ';') -notcontains $dir) {
    [Environment]::SetEnvironmentVariable('Path', "$path;$dir", 'User')
    Write-Host "Added $dir to your PATH (open a new terminal to use 'mdeer')."
}

& $exe --register
Write-Host "Installed MDeer to $exe"
