# Builds the custom Sciter release of the current checkout into custom\out\dist.
# See custom\README.md.
param(
    [string]$SigningKey = 'C:\work\keys\rustdesk-custom-sign.key',
    [string]$Config = "$PSScriptRoot\config.json",
    [string]$Tools = 'C:\work'
)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot
$out = "$PSScriptRoot\out"
$cache = "$Tools\cache"
$python = "$Tools\python\python.exe"

$env:CARGO_HOME = "$Tools\rust\cargo"
$env:RUSTUP_HOME = "$Tools\rust\rustup"
$env:VCPKG_ROOT = "$Tools\vcpkg"
$env:LIBCLANG_PATH = "$Tools\LLVM\bin"
$env:Path = "$Tools\rust\cargo\bin;$env:Path"
# CI builds Sciter with 1.75: newer versions change the i128 ABI that sciter-rs relies on.
$env:RUSTUP_TOOLCHAIN = '1.75'
$env:PYTHONUTF8 = '1'

function Run([string]$exe) {
    & $exe @args
    if ($LASTEXITCODE -ne 0) { throw "$exe $args failed with exit code $LASTEXITCODE" }
}

function Get-Cached([string]$url, [string]$name) {
    $file = "$cache\$name"
    if (-not (Test-Path $file)) {
        New-Item -ItemType Directory -Force $cache | Out-Null
        $ProgressPreference = 'SilentlyContinue'
        Invoke-WebRequest $url -OutFile "$file.part"
        Move-Item "$file.part" $file
    }
    $file
}

$cfg = Get-Content $Config -Raw -Encoding utf8
if ($cfg -match 'example\.com|SERVER_PUBLIC_KEY') { throw "$Config still has placeholder values" }
$appName = ($cfg | ConvertFrom-Json).'app-name'
if (-not $appName -or $appName -eq 'RustDesk') {
    throw "app-name in $Config must be set and differ from RustDesk, or the official updater stays enabled"
}
if (-not (Test-Path $SigningKey)) { throw "signing key $SigningKey not found" }
$version = (Select-String -Path "$root\Cargo.toml" -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches.Groups[1].Value

Push-Location $root
try {
    Run git submodule update --init --recursive
    Run $python res\inline-sciter.py
    Run cargo build --locked --release --features inline

    $rel = "$out\Release"
    if (Test-Path $rel) { Remove-Item $rel -Recurse -Force }
    New-Item -ItemType Directory -Force $rel, "$out\dist" | Out-Null
    # The packer unpacks into %LOCALAPPDATA%\<exe name without extension>.
    $exeName = "$($appName.ToLower()).exe"
    Copy-Item target\release\rustdesk.exe "$rel\$exeName"
    Copy-Item (Get-Cached 'https://raw.githubusercontent.com/c-smile/sciter-sdk/master/bin.win/x64/sciter.dll' 'sciter-x64.dll') "$rel\sciter.dll"
    $zip = Get-Cached 'https://github.com/rustdesk-org/rdev/releases/download/usbmmidd_v2/usbmmidd_v2.zip' 'usbmmidd_v2.zip'
    Expand-Archive $zip $rel
    Remove-Item "$rel\usbmmidd_v2\deviceinstaller.exe", "$rel\usbmmidd_v2\usbmmidd.bat"
    Run $python custom\sign.py sign $SigningKey $Config "$rel\custom.txt"

    # Same as CI: the packer's manifest must not declare dpiAware.
    $manifest = 'res\manifest.xml'
    $text = [IO.File]::ReadAllText("$root\$manifest")
    $nl = if ($text.Contains("`r`n")) { "`r`n" } else { "`n" }
    [IO.File]::WriteAllText("$root\$manifest", (($text -split "`r?`n" | Where-Object { $_ -notmatch 'dpiAware' }) -join $nl))
    try {
        Push-Location libs\portable
        try { Run $python generate.py -f $rel -o . -e "$rel\$exeName" } finally { Pop-Location }
    } finally {
        Run git checkout -- $manifest
        Remove-Item libs\portable\app_metadata.toml -ErrorAction SilentlyContinue
    }

    $dist = "$out\dist\$($appName.ToLower())-$version-x86_64-sciter.exe"
    Copy-Item target\release\rustdesk-portable-packer.exe $dist -Force
    Write-Host "Built $dist"

    # Unpacked variant for shipping in our own folder: no virtual display driver, no unpacking.
    $zip = [IO.Path]::ChangeExtension($dist, '.zip')
    Compress-Archive -Path "$rel\$exeName", "$rel\sciter.dll", "$rel\custom.txt" -DestinationPath $zip -Force
    Write-Host "Built $zip"
} finally {
    Pop-Location
}
