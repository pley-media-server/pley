# Verifies the licensing rules (see LICENSING.md):
#  * the server (pley-server), the transcoder (pley-transcoder) and Pley for
#    LG TV (pley-lg, the workspace in apps/webos), all under LICENSE
#    (AGPL-3.0-only), must not depend on any crate that is only available
#    under a copyleft license ((L/A)GPL and the like);
#  * the server must not depend on the transcoder crate: it is a separate
#    program, spoken to only over HTTP.
# Walks each workspace's resolved dependency graph from `cargo metadata`
# (dev-dependencies excluded), for every platform: the LG TV app's graph is
# its TV binary's and its desktop simulator's.
# Exit code 1 on violations.

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent

function Get-Metadata([string]$dir) {
    Push-Location $dir
    try {
        $json = cargo metadata --format-version 1 --quiet
        if ($LASTEXITCODE -ne 0) { throw "cargo metadata failed in $dir" }
        return $json | ConvertFrom-Json
    } finally {
        Pop-Location
    }
}

function Get-Closure($meta, [string]$name) {
    $packages = @{}
    foreach ($p in $meta.packages) { $packages[$p.id] = $p }
    $nodes = @{}
    foreach ($n in $meta.resolve.nodes) { $nodes[$n.id] = $n }
    $start = ($meta.packages | Where-Object { $_.name -eq $name -and -not $_.source } | Select-Object -First 1).id
    if (-not $start) { throw "no package $name in the workspace" }
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

$workspaces = @(
    @{ Dir = $root; Programs = "pley-server", "pley-transcoder" },
    # A workspace of its own (see the root Cargo.toml).
    @{ Dir = Join-Path $root "apps/webos"; Programs = @("pley-lg") }
)

$failed = $false

foreach ($workspace in $workspaces) {
    $meta = Get-Metadata $workspace.Dir
    # Pley's own crates are under LICENSE themselves; the rule is about the
    # crates they use.
    $own = @{}
    foreach ($id in $meta.workspace_members) { $own[$id] = $true }
    foreach ($program in $workspace.Programs) {
        $closure = Get-Closure $meta $program
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
}

if ($failed) {
    Write-Host "License check FAILED" -ForegroundColor Red
    exit 1
}
Write-Host "License check passed" -ForegroundColor Green
