# Generate rust-project.json for rust-analyzer using Buck2 BXL
$ErrorActionPreference = "Stop"

$bxlRaw = & buck2 --% bxl prelude//rust/rust-analyzer/resolve_deps.bxl:resolve_targets -- --targets root//crates/core:core

# Check if stdout contains direct JSON or a path to the generated JSON artifact
$jsonPath = $bxlRaw | Where-Object { $_ -match '\.json$' } | Select-Object -Last 1
$projectJson = $null

if ($jsonPath -and (Test-Path $jsonPath.Trim())) {
    $projectJson = Get-Content $jsonPath.Trim() -Raw
} else {
    $jsonLine = $bxlRaw | Where-Object { $_ -match '^\s*\{.*"crates".*\}' } | Select-Object -First 1
    if ($jsonLine) {
        $projectJson = $jsonLine
    }
}

if (-not $projectJson) {
    Write-Error "Failed to locate crate graph JSON from buck2 bxl output."
}

$project = $projectJson | ConvertFrom-Json
$sysroot = (rustc --print sysroot).Trim()
$project | Add-Member -MemberType NoteProperty -Name "sysroot" -Value $sysroot -Force

$repoRoot = Split-Path $PSScriptRoot -Parent
$outFile = Join-Path $repoRoot "rust-project.json"
$project | ConvertTo-Json -Depth 15 | Set-Content $outFile -Encoding utf8
Write-Host "Generated rust-project.json successfully at $outFile"
