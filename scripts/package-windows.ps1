param(
    [string]$InnoSetupPath
)

$ErrorActionPreference = 'Stop'

$repositoryRoot = Split-Path -Parent $PSScriptRoot
$manifestPath = Join-Path $repositoryRoot 'Cargo.toml'
$manifest = Get-Content -LiteralPath $manifestPath -Raw
$versionMatch = [regex]::Match($manifest, '(?m)^version\s*=\s*"([^"]+)"')
if (-not $versionMatch.Success) {
    throw 'Could not read the package version from Cargo.toml.'
}

$version = $versionMatch.Groups[1].Value
$packageName = "Lilo-$version-windows-x64"
$distRoot = Join-Path $repositoryRoot 'dist'
$packageDirectory = Join-Path $distRoot $packageName
$archivePath = Join-Path $distRoot "$packageName.zip"
$installerPath = Join-Path $distRoot "Lilo-$version-windows-x64-setup.exe"
$resolvedDistRoot = [System.IO.Path]::GetFullPath($distRoot)
$resolvedPackageDirectory = [System.IO.Path]::GetFullPath($packageDirectory)
if ([System.IO.Path]::GetDirectoryName($resolvedPackageDirectory) -ne $resolvedDistRoot) {
    throw 'Refusing to package outside the repository dist directory.'
}

Push-Location $repositoryRoot
try {
    cargo build --release --locked
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build failed with exit code $LASTEXITCODE."
    }

    $executable = Join-Path $repositoryRoot 'target\release\Lilo.exe'
    if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) {
        throw 'The release executable was not produced.'
    }

    New-Item -ItemType Directory -Path $resolvedDistRoot -Force | Out-Null
    if (Test-Path -LiteralPath $packageDirectory) {
        Remove-Item -LiteralPath $packageDirectory -Recurse -Force
    }
    if (Test-Path -LiteralPath $archivePath) {
        Remove-Item -LiteralPath $archivePath -Force
    }
    if (Test-Path -LiteralPath $installerPath) {
        Remove-Item -LiteralPath $installerPath -Force
    }
    New-Item -ItemType Directory -Path $packageDirectory | Out-Null

    Copy-Item -LiteralPath $executable -Destination $packageDirectory
    foreach ($document in @('README.md', 'ROADMAP.md', 'CHANGELOG.md', 'RELEASE.md', 'PRIVACY.md', 'LICENSE')) {
        Copy-Item -LiteralPath (Join-Path $repositoryRoot $document) -Destination $packageDirectory
    }

    Compress-Archive -LiteralPath $packageDirectory -DestinationPath $archivePath -CompressionLevel Optimal
    $checksum = Get-FileHash -LiteralPath $archivePath -Algorithm SHA256
    "$($checksum.Hash)  $([System.IO.Path]::GetFileName($archivePath))" |
        Set-Content -LiteralPath "$archivePath.sha256" -Encoding ascii

    if ([string]::IsNullOrWhiteSpace($InnoSetupPath)) {
        $isccCommand = Get-Command 'ISCC.exe' -ErrorAction SilentlyContinue
        if ($null -ne $isccCommand) {
            $InnoSetupPath = $isccCommand.Source
        }
        else {
            $innoSetupCandidates = @(
                'C:\Program Files (x86)\Inno Setup 6\ISCC.exe',
                (Join-Path $env:LOCALAPPDATA 'Programs\Inno Setup 6\ISCC.exe')
            )
            $InnoSetupPath = $innoSetupCandidates |
                Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } |
                Select-Object -First 1
        }
    }

    if (-not (Test-Path -LiteralPath $InnoSetupPath -PathType Leaf)) {
        throw "Inno Setup 6 was not found. Install it with 'winget install JRSoftware.InnoSetup' or pass -InnoSetupPath."
    }

    $innoScript = Join-Path $repositoryRoot 'installer\Lilo.iss'
    & $InnoSetupPath "/DMyAppVersion=$version" "/DMySourceExe=$executable" "/DMyOutputDir=$distRoot" $innoScript
    if ($LASTEXITCODE -ne 0) {
        throw "Inno Setup failed with exit code $LASTEXITCODE."
    }
    if (-not (Test-Path -LiteralPath $installerPath -PathType Leaf)) {
        throw 'Inno Setup did not produce the expected installer.'
    }

    $installerChecksum = Get-FileHash -LiteralPath $installerPath -Algorithm SHA256
    "$($installerChecksum.Hash)  $([System.IO.Path]::GetFileName($installerPath))" |
        Set-Content -LiteralPath "$installerPath.sha256" -Encoding ascii

    Write-Host "Created $archivePath"
    Write-Host "SHA256: $($checksum.Hash)"
    Write-Host "Created $installerPath"
    Write-Host "SHA256: $($installerChecksum.Hash)"
}
finally {
    Pop-Location
}
