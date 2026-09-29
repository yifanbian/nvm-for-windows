param(
    [string]$NvmHome
)

$ErrorActionPreference = 'Stop'

if ([string]::IsNullOrWhiteSpace($NvmHome)) {
    if (-not [string]::IsNullOrWhiteSpace($env:NVM_HOME)) {
        $NvmHome = $env:NVM_HOME
    } elseif (-not [string]::IsNullOrWhiteSpace($env:LOCALAPPDATA)) {
        $NvmHome = Join-Path $env:LOCALAPPDATA 'nvm-rs'
    } else {
        throw 'Set NVM_HOME or LOCALAPPDATA before installing.'
    }
}

$NvmHome = [IO.Path]::GetFullPath($NvmHome)
$installedBinary = Join-Path $NvmHome 'nvm-for-windows-rs.exe'
$temporaryDirectory = Join-Path ([IO.Path]::GetTempPath()) "nvm-for-windows-$([guid]::NewGuid().ToString('N'))"
$downloadedBinary = Join-Path $temporaryDirectory 'nvm-for-windows-rs.exe'
try {
    New-Item -ItemType Directory -Path $temporaryDirectory -Force | Out-Null
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    $headers = @{
        Accept = 'application/vnd.github+json'
        'X-GitHub-Api-Version' = '2022-11-28'
        'User-Agent' = 'nvm-for-windows-installer'
    }
    try {
        $release = Invoke-RestMethod -Uri 'https://api.github.com/repos/yifanbian/nvm-for-windows/releases/latest' -Headers $headers
    } catch {
        throw 'Could not retrieve the latest release. Confirm the repository is public and has a published v* release.'
    }
    $asset = $release.assets | Where-Object name -eq 'nvm-for-windows-rs.exe' | Select-Object -First 1
    if (-not $asset) {
        throw "Latest release $($release.tag_name) does not contain nvm-for-windows-rs.exe."
    }

    Write-Host "Downloading nvm-for-windows $($release.tag_name)"
    Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $downloadedBinary -UseBasicParsing
    if (-not (Test-Path $downloadedBinary -PathType Leaf)) {
        throw 'The release binary download did not produce a file.'
    }
    if ($asset.digest -and $asset.digest.StartsWith('sha256:')) {
        $expectedHash = $asset.digest.Substring('sha256:'.Length)
        $actualHash = (Get-FileHash -Path $downloadedBinary -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($actualHash -ne $expectedHash.ToLowerInvariant()) {
            throw 'The downloaded release binary failed SHA-256 verification.'
        }
    }

    New-Item -ItemType Directory -Path $NvmHome -Force | Out-Null
    Copy-Item -Path $downloadedBinary -Destination $installedBinary -Force
} finally {
    if (Test-Path $temporaryDirectory) {
        Remove-Item -Path $temporaryDirectory -Recurse -Force -ErrorAction SilentlyContinue
    }
}

$env:NVM_HOME = $NvmHome

Write-Host "Installed manager at $installedBinary"
Write-Host 'Creating command shims and configuring the current-user PATH'
$setupOutput = & $installedBinary setup 2>&1
$setupExitCode = $LASTEXITCODE
$setupOutput | ForEach-Object { Write-Host $_ }
if ($setupExitCode -ne 0) {
    throw "nvm setup failed with exit code $setupExitCode. Enable Windows Developer Mode or run PowerShell elevated, then retry."
}

Write-Host "Installation complete. Restart PowerShell, then run: nvm install lts/*"
