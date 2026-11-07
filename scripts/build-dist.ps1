# Build a standalone Browsitory distributable for Windows.
#
# Produces .msi and .exe (NSIS) installers via `tauri build`, run with the @tauri-apps/cli that
# frontend/pnpm-lock.yaml locks (the one the release build runs, #430). Every installer carries
# THIRD_PARTY_NOTICES.md as a Tauri resource (tauri.conf.json); a copy is also put next to them.
#
# Usage: scripts\build-dist.ps1 [extra tauri build args]
$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path "$PSScriptRoot\.."

if (-not (Get-Command pnpm -ErrorAction SilentlyContinue)) {
    Write-Error "pnpm not found (https://pnpm.io)"
    exit 1
}

if (-not (Get-Command node -ErrorAction SilentlyContinue)) {
    Write-Error "node not found (https://nodejs.org)"
    exit 1
}

Write-Host "==> Installing frontend dependencies"
pnpm --dir "$repoRoot\frontend" install --frozen-lockfile
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

# Run with node from crates\tauri-app, where the CLI finds tauri.conf.json (`pnpm exec` would
# run it in frontend\).
$tauriCli = Join-Path $repoRoot "frontend\node_modules\@tauri-apps\cli\tauri.js"

Write-Host "==> Building distributable"
Push-Location "$repoRoot\crates\tauri-app"
try {
    node $tauriCli --version
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    node $tauriCli build @args
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
