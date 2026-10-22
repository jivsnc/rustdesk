# Builds the custom Sciter release of the current checkout into custom\out\dist.
# See custom\README.md.
param(
    [string]$SigningKey = 'C:\work\keys\rustdesk-custom-sign.key',
    [string]$Config = 'C:\work\keys\sncdesk-config.json',
    [string]$Tools = 'C:\work',
    # x86 is the build for 32-bit Windows (including Windows 7 x86); it also runs on x64.
    [ValidateSet('x64', 'x86')]
    [string]$Arch = 'x64'
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
$archName = 'x86_64'
$sciterUrl = 'https://raw.githubusercontent.com/c-smile/sciter-sdk/master/bin.win/x64/sciter.dll'
$target = "$root\target"
if ($Arch -eq 'x86') {
    # Same as CI: sciter-rs' x86 branch needs nightly (abi_thiscall), and an i686 host
    # toolchain so bindgen in build scripts can load the 32-bit libclang.
    $env:RUSTUP_TOOLCHAIN = 'nightly-2023-10-13-i686-pc-windows-msvc'
    $env:LIBCLANG_PATH = "$Tools\LLVM-x86\bin"
    $archName = 'x86'
    $sciterUrl = 'https://raw.githubusercontent.com/c-smile/sciter-sdk/master/bin.win/x32/sciter.dll'
    # A separate target dir keeps the x64 build cache intact.
    $target = "$out\target-x86"
    $env:CARGO_TARGET_DIR = $target
    if (-not (Test-Path "$env:VCPKG_ROOT\installed\x86-windows-static")) { throw "vcpkg x86-windows-static packages not installed, see custom\README.md" }
}

# No param block: a declared parameter would swallow tool flags like -e by prefix match.
function Run {
    $exe = $args[0]
    $rest = @($args | Select-Object -Skip 1)
    & $exe @rest
    if ($LASTEXITCODE -ne 0) { throw "$args failed with exit code $LASTEXITCODE" }
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

if (-not (Test-Path $Config)) { throw "config $Config not found: copy custom\config.example.json there and fill it in" }
$cfg = Get-Content $Config -Raw -Encoding utf8
if ($cfg -match 'example\.com|SERVER_PUBLIC_KEY') { throw "$Config still has placeholder values" }
$appName = ($cfg | ConvertFrom-Json).'app-name'
if (-not $appName -or $appName -eq 'RustDesk') {
    throw "app-name in $Config must be set and differ from RustDesk, or the official updater stays enabled"
}
if (-not (Test-Path $SigningKey)) { throw "signing key $SigningKey not found" }
$started = Get-Date
$version = (Select-String -Path "$root\Cargo.toml" -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches.Groups[1].Value

Push-Location $root
try {
    Run git submodule update --init --recursive
    Run $python res\inline-sciter.py
    if ($Arch -eq 'x86') {
        # Same as CI: switch sciter-rs to its x86 branch for this build only.
        $toml = [IO.File]::ReadAllText("$root\Cargo.toml")
        [IO.File]::WriteAllText("$root\Cargo.toml", $toml.Replace('branch = "dyn"', 'branch = "dyn_x86"'))
        try {
            Run cargo update -p sciter-rs --precise 674e07d3066ca9a92ced3816203ab6b652629d1e
            Run cargo build --locked --release --features inline
        } finally {
            Run git checkout -- Cargo.toml Cargo.lock
        }
    } else {
        Run cargo build --locked --release --features inline
    }

    $rel = "$out\Release"
    if (Test-Path $rel) { Remove-Item $rel -Recurse -Force }
    New-Item -ItemType Directory -Force $rel, "$out\dist" | Out-Null
    # The packer unpacks into %LOCALAPPDATA%\<exe name without extension>.
    $exeName = "$($appName.ToLower()).exe"
    Copy-Item "$target\release\rustdesk.exe" "$rel\$exeName"
    Copy-Item (Get-Cached $sciterUrl "sciter-$Arch.dll") "$rel\sciter.dll"
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

    $packer = Get-Item "$target\release\rustdesk-portable-packer.exe"
    if ($packer.LastWriteTime -lt $started) { throw "$($packer.FullName) was not rebuilt" }
    $dist = "$out\dist\$($appName.ToLower())-$version-$archName-sciter.exe"
    Copy-Item $packer $dist -Force
    Write-Host "Built $dist"

    # Unpacked variant for shipping in our own folder: no virtual display driver, no unpacking.
    $zip = [IO.Path]::ChangeExtension($dist, '.zip')
    Compress-Archive -Path "$rel\$exeName", "$rel\sciter.dll", "$rel\custom.txt" -DestinationPath $zip -Force
    Write-Host "Built $zip"
} finally {
    Pop-Location
}
