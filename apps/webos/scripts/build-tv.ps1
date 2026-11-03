# Builds Pley for LG TV for the TV and packs apps/webos/out/app.pley.tv_<version>_arm.ipk,
# in Docker: the webOS toolchain runs on Linux only. The image is Ubuntu with Rust and the
# toolchain that .github/workflows/lg.yml pins (WEBOS_SDK_URL, WEBOS_SDK_SHA256), and the
# container runs the same tasks as CI's TV job, so the two builds differ only in the machine.
#
#   pwsh apps/webos/scripts/build-tv.ps1             build, check, pack and verify
#   pwsh apps/webos/scripts/build-tv.ps1 -Shell      a shell in the build container instead
#   pwsh apps/webos/scripts/build-tv.ps1 -Rebuild    rebuild the image first (a newer Rust)
#
# Cargo's downloads and the build's target folder stay in Docker volumes (pley-lg-cargo,
# pley-lg-target), so later builds are quick.

[CmdletBinding()]
param(
    [switch]$Shell,
    [switch]$Rebuild
)

$ErrorActionPreference = "Stop"
$webos = Split-Path $PSScriptRoot -Parent
$repo = Split-Path (Split-Path $webos -Parent) -Parent

if (-not (Get-Command docker -ErrorAction SilentlyContinue)) {
    Write-Host "Docker isn't installed. The TV build needs Linux: install Docker, or let CI build it (.github/workflows/lg.yml)." -ForegroundColor Yellow
    exit 1
}
docker info *> $null
if ($LASTEXITCODE -ne 0) {
    Write-Host "Docker isn't running: start it (Docker Desktop) and run this again, or let CI build it (.github/workflows/lg.yml)." -ForegroundColor Yellow
    exit 1
}

$workflow = Get-Content (Join-Path $repo ".github/workflows/lg.yml") -Raw
$url = [regex]::Match($workflow, '(?m)^\s*WEBOS_SDK_URL:\s*(\S+)').Groups[1].Value
$sha = [regex]::Match($workflow, '(?m)^\s*WEBOS_SDK_SHA256:\s*([0-9a-f]{64})\s*$').Groups[1].Value
if (-not $url -or -not $sha) { throw ".github/workflows/lg.yml doesn't pin WEBOS_SDK_URL and WEBOS_SDK_SHA256" }

# Everything lives under /builder, a folder no path in the binary is remapped to, so
# `cargo xtask check` can tell a path that escaped --remap-path-prefix.
$dockerfile = @'
FROM ubuntu:24.04
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl gcc libc6-dev \
 && rm -rf /var/lib/apt/lists/*
ARG SDK_URL
ARG SDK_SHA256
RUN curl -fsSL --retry 3 -o /tmp/sdk.tar.gz "$SDK_URL" \
 && echo "$SDK_SHA256  /tmp/sdk.tar.gz" | sha256sum -c - \
 && mkdir -p /builder/sdk \
 && tar -xzf /tmp/sdk.tar.gz -C /builder/sdk \
 && rm /tmp/sdk.tar.gz \
 && /builder/sdk/arm-webos-linux-gnueabi_sdk-buildroot/relocate-sdk.sh
ENV RUSTUP_HOME=/builder/rustup CARGO_HOME=/builder/cargo
ENV PATH=/builder/sdk/arm-webos-linux-gnueabi_sdk-buildroot/bin:/builder/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
RUN curl -fsSL --proto '=https' --tlsv1.2 https://sh.rustup.rs \
  | sh -s -- -y --no-modify-path --profile minimal --default-toolchain stable --target armv7-unknown-linux-gnueabi
'@

# The image is named after the toolchain it holds, so a new pin builds a new one.
$image = "pley-lg-build:" + $sha.Substring(0, 12)
docker image inspect $image *> $null
if ($Rebuild -or $LASTEXITCODE -ne 0) {
    $options = @("build", "--tag", $image, "--build-arg", "SDK_URL=$url", "--build-arg", "SDK_SHA256=$sha")
    if ($Rebuild) { $options += @("--pull", "--no-cache") }
    $dockerfile | docker @options -
    if ($LASTEXITCODE -ne 0) { throw "docker build failed" }
}

# The container has no git: the build learns its commit from here.
$commit = git -C $repo rev-parse --short=9 HEAD 2>$null
$run = @(
    "run", "--rm",
    "--volume", "${repo}:/builder/repo",
    "--volume", "pley-lg-cargo:/builder/cargo/registry",
    "--volume", "pley-lg-target:/builder/target",
    "--env", "CARGO_TARGET_DIR=/builder/target",
    "--env", "PLEY_COMMIT=$commit",
    "--workdir", "/builder/repo/apps/webos"
)
if ($Shell) {
    docker @run --interactive --tty $image bash
    exit $LASTEXITCODE
}
docker @run $image sh -c "cargo xtask build --locked && cargo xtask check && cargo xtask ipk --no-build && cargo xtask ipk-verify"
if ($LASTEXITCODE -ne 0) { throw "the TV build failed" }
Get-ChildItem (Join-Path $webos "out") -Filter *.ipk | ForEach-Object { Write-Host "$($_.FullName) ($($_.Length) bytes)" -ForegroundColor Green }
