# Build a standalone Browsitory distributable for Windows.
#
# Produces .msi and .exe (NSIS) installers via `cargo tauri build`.
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
} finally {
    Pop-Location
}

Write-Host "==> Done. Bundles in target\release\bundle\"
