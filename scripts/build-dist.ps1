# Build a standalone Browsitory distributable for Windows.
#
# Produces .msi and .exe (NSIS) installers via `cargo tauri build`. Every installer carries
# THIRD_PARTY_NOTICES.md as a Tauri resource (tauri.conf.json); a copy is also put next to them.
#
# Usage: scripts\build-dist.ps1 [extra cargo-tauri build args]
$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path "$PSScriptRoot\.."

if (-not (Get-Command pnpm -ErrorAction SilentlyContinue)) {
    Write-Error "pnpm not found (https://pnpm.io)"
    exit 1
}

cargo tauri --version | Out-Null
if ($LASTEXITCODE -ne 0) {
    Write-Error "tauri-cli not found; install with: cargo install tauri-cli"
    exit 1
}

Write-Host "==> Installing frontend dependencies"
pnpm --dir "$repoRoot\frontend" install --frozen-lockfile

Write-Host "==> Building distributable"
Push-Location "$repoRoot\crates\tauri-app"
try {
    cargo tauri build @args
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally {
    Pop-Location
}

# `--target <triple>` (or `--target=<triple>`) moves the bundles to target\<triple>\release\.
$bundleDir = "target\release\bundle"
for ($i = 0; $i -lt $args.Count; $i++) {
    if ($args[$i] -eq "--target" -and $i + 1 -lt $args.Count) {
        $bundleDir = "target\$($args[$i + 1])\release\bundle"
    } elseif ($args[$i] -like "--target=*") {
        $bundleDir = "target\$($args[$i].Substring(9))\release\bundle"
    }
}
Copy-Item "$repoRoot\THIRD_PARTY_NOTICES.md" "$repoRoot\$bundleDir\"

Write-Host "==> Done. Bundles and THIRD_PARTY_NOTICES.md in $bundleDir\"
