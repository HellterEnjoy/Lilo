param(
    [Parameter(Mandatory = $true)]
    [string]$Version,
    [Parameter(Mandatory = $true)]
    [string]$Repository,
    [string]$Tag = "v$Version",
    [string]$Token = $env:WINGET_GITHUB_TOKEN,
    [string]$WinGetCreatePath = (Join-Path $env:RUNNER_TEMP 'wingetcreate.exe')
)

$ErrorActionPreference = 'Stop'

if ($Version -notmatch '^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$') {
    throw "Version '$Version' is invalid. Pass it without the v prefix."
}
if ($Tag -ne "v$Version") {
    throw "Tag '$Tag' does not match version '$Version'."
}
if ([string]::IsNullOrWhiteSpace($Token)) {
    throw 'WINGET_GITHUB_TOKEN is required for WinGet submission.'
}
if ([string]::IsNullOrWhiteSpace($Repository) -or $Repository -notmatch '^[^/]+/[^/]+$') {
    throw "Repository '$Repository' must use the owner/name format."
}

if (-not (Test-Path -LiteralPath $WinGetCreatePath -PathType Leaf)) {
    Invoke-WebRequest 'https://aka.ms/wingetcreate/latest' -OutFile $WinGetCreatePath
}

$installerUrl = "https://github.com/$Repository/releases/download/$Tag/Lilo-$Version-windows-x64-setup.exe"
$releaseNotesUrl = "https://github.com/$Repository/releases/tag/$Tag"

& $WinGetCreatePath update HellterEnjoy.Lilo `
    --version $Version `
    --urls $installerUrl `
    --release-notes-url $releaseNotesUrl `
    --submit `
    --token $Token `
    --no-open
if ($LASTEXITCODE -ne 0) {
    throw "WinGet submission failed with exit code $LASTEXITCODE."
}
