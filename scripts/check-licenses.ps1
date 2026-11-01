# Verifies the licensing rules (see LICENSING.md):
#  * the server (pley-server) and the transcoder (pley-transcoder), both
#    under LICENSE (AGPL-3.0-only), must not depend on any crate that is only
#    available under a copyleft license ((L/A)GPL and the like);
#  * the server must not depend on the transcoder crate: it is a separate
#    program, spoken to only over HTTP.
# Walks the resolved dependency graph from `cargo metadata` (dev-dependencies excluded).
# Exit code 1 on violations.

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
Push-Location $root
try {
    $meta = cargo metadata --format-version 1 --quiet | ConvertFrom-Json
} finally {
    Pop-Location
}

$packages = @{}
foreach ($p in $meta.packages) { $packages[$p.id] = $p }
$nodes = @{}
foreach ($n in $meta.resolve.nodes) { $nodes[$n.id] = $n }
# Pley's own crates are under LICENSE themselves; the rule is about the
# crates they use.
$own = @{}
foreach ($id in $meta.workspace_members) { $own[$id] = $true }

function Get-Closure([string]$name) {
    $start = ($meta.packages | Where-Object { $_.name -eq $name } | Select-Object -First 1).id
    $seen = @{}
    $queue = [System.Collections.Generic.Queue[string]]::new()
    $queue.Enqueue($start)
    while ($queue.Count -gt 0) {
        $id = $queue.Dequeue()
        if ($seen.ContainsKey($id)) { continue }
        $seen[$id] = $true
        foreach ($dep in $nodes[$id].deps) {
            # dep_kinds: kind = null (normal), "build" or "dev"
            $runtime = @($dep.dep_kinds | Where-Object { $_.kind -ne "dev" }).Count -gt 0
            if ($runtime) { $queue.Enqueue($dep.pkg) }
        }
    }
    return $seen.Keys | ForEach-Object { $packages[$_] }
}

# A license expression is "copyleft-only" when every alternative is copyleft.
function Test-CopyleftOnly([string]$expr) {
    if (-not $expr) { return $false }
    $alternatives = $expr -split '\s+OR\s+|/'
    foreach ($alt in $alternatives) {
        if ($alt -notmatch 'GPL|SSPL|EUPL|OSL') { return $false }
    }
    return $true
}

$failed = $false

foreach ($program in "pley-server", "pley-transcoder") {
    $closure = Get-Closure $program
    Write-Host "${program}: $($closure.Count) crates in the runtime dependency graph"
    foreach ($p in $closure) {
        if ($program -eq "pley-server" -and $p.name -eq "pley-transcoder") {
            Write-Host "  ERROR: pley-server depends on pley-transcoder (a separate program)" -ForegroundColor Red
            $failed = $true
        }
        if ($own.ContainsKey($p.id)) { continue }
        if (Test-CopyleftOnly $p.license) {
            Write-Host "  ERROR: $($p.name) $($p.version) is $($p.license)" -ForegroundColor Red
            $failed = $true
        }
        if (-not $p.license -and -not $p.license_file -and $p.source) {
            Write-Host "  WARN: $($p.name) $($p.version) declares no license" -ForegroundColor Yellow
        }
    }
    $licenses = $closure | Group-Object { if ($_.license) { $_.license } else { "(file) $($_.license_file)" } } | Sort-Object Count -Descending
    Write-Host "Licenses used by ${program}:"
    foreach ($l in $licenses) { Write-Host ("  {0,4}  {1}" -f $l.Count, $l.Name) }
}

if ($failed) {
    Write-Host "License check FAILED" -ForegroundColor Red
    exit 1
}
Write-Host "License check passed" -ForegroundColor Green
