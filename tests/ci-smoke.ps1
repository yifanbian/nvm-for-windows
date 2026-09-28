$ErrorActionPreference = 'Stop'

$env:NVM_HOME = Join-Path $env:RUNNER_TEMP 'nvm-ci-home'
$script:NvmCommand = 'nvm-for-windows-rs.exe'
$script:ShimDir = Join-Path $env:NVM_HOME 'shims'

function Invoke-Nvm {
    param([string[]]$NvmArgs)

    $output = & $script:NvmCommand @NvmArgs 2>&1
    $exitCode = $LASTEXITCODE
    if ($exitCode -ne 0) {
        throw "nvm $($NvmArgs -join ' ') failed with exit code ${exitCode}: $($output | Out-String)"
    }
    return ($output | Out-String).Trim()
}

function Invoke-Shim {
    param(
        [Parameter(Mandatory)] [string] $Name,
        [string[]] $ToolArgs
    )

    $shim = Join-Path $script:ShimDir $Name
    $output = & $shim @ToolArgs 2>&1
    $exitCode = $LASTEXITCODE
    if ($exitCode -ne 0) {
        throw "$Name $($ToolArgs -join ' ') failed with exit code ${exitCode}: $($output | Out-String)"
    }
    return ($output | Out-String).Trim()
}

function Assert-Equal {
    param([string]$Actual, [string]$Expected, [string]$Description)

    if ($Actual -ne $Expected) {
        throw "$Description`: expected '$Expected', got '$Actual'"
    }
}

$versionOutput = Invoke-Nvm @('--version')
if ([string]::IsNullOrWhiteSpace($versionOutput)) {
    throw 'The downloaded executable did not print its version.'
}
Invoke-Nvm @('--help') | Out-Null

$install24 = Invoke-Nvm @('install', '24')
$version24 = ($install24 -split '\s+')[-1]
$install26 = Invoke-Nvm @('install', '26')
$version26 = ($install26 -split '\s+')[-1]
if ($version24 -notmatch '^v24\.' -or $version26 -notmatch '^v26\.') {
    throw "Unexpected installed versions: $version24 and $version26"
}

$list = Invoke-Nvm @('list')
if ($list -notmatch [regex]::Escape($version24) -or $list -notmatch [regex]::Escape($version26)) {
    throw "Installed versions were missing from nvm list: $list"
}

$ltsInstall = Invoke-Nvm @('install', 'lts/krypton')
if ($ltsInstall -notmatch [regex]::Escape($version24)) {
    throw "lts/krypton did not resolve to installed Node.js $version24`: $ltsInstall"
}

Invoke-Nvm @('setup') | Out-Null
if (-not (Test-Path $script:ShimDir)) {
    throw "Shim directory was not created: $script:ShimDir"
}
$env:PATH = "$script:ShimDir;$env:PATH"

foreach ($name in @('nvm.exe', 'node.exe', 'npm.exe', 'npx.exe', 'corepack.exe')) {
    $shimPath = Join-Path $script:ShimDir $name
    if (-not (Test-Path $shimPath) -or (Get-Item $shimPath).LinkType -ne 'SymbolicLink') {
        throw "Expected a file symlink at $shimPath"
    }
}

$resolvedNode = (where.exe node.exe | Select-Object -First 1).Trim()
Assert-Equal $resolvedNode (Join-Path $script:ShimDir 'node.exe') 'PATH node resolution'

Invoke-Nvm @('use', '24') | Out-Null
Assert-Equal (Invoke-Nvm @('current')) $version24 'Default version selection'
if ((Invoke-Nvm @('which')) -ne (Join-Path $env:NVM_HOME "versions\$version24\node.exe")) {
    throw 'nvm which did not return the selected node.exe.'
}
Assert-Equal (Invoke-Shim 'node.exe' @('--version')) $version24 'node shim version'
if ([string]::IsNullOrWhiteSpace((Invoke-Shim 'npm.exe' @('--version'))) -or
    [string]::IsNullOrWhiteSpace((Invoke-Shim 'npx.exe' @('--version'))) -or
    [string]::IsNullOrWhiteSpace((Invoke-Shim 'corepack.exe' @('--version')))) {
    throw 'One or more bundled package-manager shims returned no version.'
}

$projectDir = Join-Path $env:RUNNER_TEMP 'nvm-project'
New-Item -ItemType Directory -Force $projectDir | Out-Null
Set-Content -Path (Join-Path $projectDir '.nvmrc') -Value '26'
$originalLocation = Get-Location
try {
    Set-Location $projectDir
    Assert-Equal (Invoke-Nvm @('current')) $version26 '.nvmrc selection'
    Assert-Equal (Invoke-Shim 'node.exe' @('--version')) $version26 '.nvmrc node shim version'
} finally {
    Set-Location $originalLocation
}

Invoke-Nvm @('use', '24') | Out-Null
Invoke-Shim 'npm.exe' @('install', '--global', 'is-number@7.0.0') | Out-Null
$globalPackage24 = Join-Path $env:NVM_HOME "globals\$version24\node_modules\is-number"
if (-not (Test-Path $globalPackage24)) {
    throw "Global npm package was not installed under $globalPackage24"
}

Invoke-Nvm @('use', '26') | Out-Null
$globalRoot26 = Invoke-Shim 'npm.exe' @('root', '--global')
$expectedRoot26 = Join-Path $env:NVM_HOME "globals\$version26\node_modules"
Assert-Equal ([IO.Path]::GetFullPath($globalRoot26)) ([IO.Path]::GetFullPath($expectedRoot26)) 'Version-specific npm global root'
if (Test-Path (Join-Path $expectedRoot26 'is-number')) {
    throw 'The Node.js 24 global package leaked into the Node.js 26 environment.'
}

Invoke-Nvm @('use', '24') | Out-Null
Invoke-Nvm @('uninstall', '24') | Out-Null
if ((Test-Path (Join-Path $env:NVM_HOME "versions\$version24")) -or (Test-Path $globalPackage24)) {
    throw 'Uninstall did not remove the Node.js version and its global packages.'
}
Assert-Equal (Invoke-Nvm @('current')) $version26 'Fallback version after uninstall'
Assert-Equal (Invoke-Shim 'node.exe' @('--version')) $version26 'node shim after uninstall'

Write-Host "Downloaded artifact passed CLI, install, setup, PATH, node/npm/npx/corepack, .nvmrc, global isolation, use, which, list, and uninstall checks ($version24, $version26)."