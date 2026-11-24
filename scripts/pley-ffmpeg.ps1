# Puts pley-ffmpeg, the FFmpeg Pley ships, in a folder: the files of its
# image in GitHub's container registry, taken without Docker and checked
# against their digests.
#
#   scripts/pley-ffmpeg.ps1 -Platform windows -Destination ffmpeg
#   scripts/pley-ffmpeg.ps1 -Platform linux -Arch arm64 -Destination ffmpeg
#
# Windows: bin\ffmpeg.exe and bin\ffprobe.exe, with FFmpeg's license and
# what it was built from, as the Windows installers carry them. Linux: the
# Docker image's FFmpeg, its usr folder and packages, the list of Debian 13
# packages its libraries need; it runs on Debian 13 only.
#
# The image is the one this checkout's recipe names (deploy/ffmpeg/tag.sh).
# The FFmpeg workflow publishes a changed recipe once it is on master: until
# then, the image -Fallback's recipe names is taken instead, saying so
# (master's, or on master the commit before).

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("windows", "linux")]
    [string]$Platform,
    [Parameter(Mandatory = $true)]
    [string]$Destination,
    [ValidateSet("amd64", "arm64")]
    [string]$Arch = "amd64",
    # The branch or commit whose recipe's image is taken when this one's isn't published.
    [string]$Fallback = "master"
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"
$root = Split-Path $PSScriptRoot -Parent
$onWindows = [System.IO.Path]::DirectorySeparatorChar -eq "\"
$repository = if ($Platform -eq "windows") { "pley-media-server/pley-ffmpeg-windows" } else { "pley-media-server/pley-ffmpeg" }

# Windows' own tar, which reads C:\ paths as paths, not as hosts as Git's does.
$tar = if ($onWindows) { Join-Path $env:SystemRoot "System32\tar.exe" } else { "tar" }

# A request tried again a few times: the registry, or the network, fails now and then.
function Invoke-Retried([scriptblock]$Request) {
    for ($attempt = 1; ; $attempt++) {
        try { return & $Request } catch {
            $status = if ($_.Exception.Response) { [int]$_.Exception.Response.StatusCode } else { 0 }
            if ($attempt -eq 4 -or ($status -ge 400 -and $status -lt 500 -and $status -ne 429)) { throw }
            Start-Sleep -Seconds (5 * $attempt)
        }
    }
}

# sh, which tag.sh needs: on PATH, or Git's on Windows.
function Get-Sh {
    $sh = Get-Command sh -ErrorAction SilentlyContinue
    if ($sh) { return $sh.Source }
    # git --exec-path is <Git>\mingw64\libexec\git-core, wherever git.exe is.
    $git = Split-Path (Split-Path (Split-Path (git --exec-path)))
    foreach ($candidate in @("bin\sh.exe", "usr\bin\sh.exe")) {
        if (Test-Path (Join-Path $git $candidate)) { return Join-Path $git $candidate }
    }
    throw "no sh: install Git for Windows"
}

# The image's tag, from tag.sh in the deploy/ffmpeg folder of a checkout.
function Get-Tag([string]$Checkout) {
    $arguments = @("$Checkout/deploy/ffmpeg/tag.sh")
    if ($Platform -eq "windows") { $arguments += "windows" }
    $tag = & (Get-Sh) @arguments
    if ($LASTEXITCODE -ne 0 -or -not $tag) { throw "tag.sh failed" }
    return "$tag".Trim()
}

$token = (Invoke-Retried { Invoke-RestMethod "https://ghcr.io/token?scope=repository:${repository}:pull" }).token
$accept = "application/vnd.oci.image.index.v1+json, application/vnd.oci.image.manifest.v1+json, " +
    "application/vnd.docker.distribution.manifest.list.v2+json, application/vnd.docker.distribution.manifest.v2+json"

# A manifest of the image, as read and as bytes, or $null where there is none (not published).
function Get-Manifest([string]$Reference) {
    try {
        $response = Invoke-Retried {
            Invoke-WebRequest -UseBasicParsing "https://ghcr.io/v2/$repository/manifests/$Reference" `
                -Headers @{ Authorization = "Bearer $token"; Accept = $accept }
        }
    } catch {
        if ($_.Exception.Response -and [int]$_.Exception.Response.StatusCode -eq 404) { return $null }
        throw
    }
    $bytes = $response.RawContentStream.ToArray()
    return @{ Read = ([System.Text.Encoding]::UTF8.GetString($bytes) | ConvertFrom-Json); Bytes = $bytes }
}

$tag = Get-Tag $root
$manifest = Get-Manifest $tag
if (-not $manifest) {
    $message = "ghcr.io/${repository}:$tag isn't published yet: taking the one $Fallback's recipe names"
    if ($env:GITHUB_ACTIONS) { Write-Host "::warning::$message" } else { Write-Warning $message }
    $other = Join-Path ([System.IO.Path]::GetTempPath()) "pley-ffmpeg-recipe-$PID"
    New-Item -ItemType Directory -Force $other | Out-Null
    # A shallow checkout (CI's) stays shallow; a whole one isn't made so.
    $depth = if ((git -C $root rev-parse --is-shallow-repository) -eq "true") { @("--depth=1") } else { @() }
    git -C $root fetch --quiet @depth origin $Fallback
    if ($LASTEXITCODE -ne 0) { throw "git fetch failed" }
    git -C $root archive -o "$other/recipe.tar" FETCH_HEAD deploy/ffmpeg
    if ($LASTEXITCODE -ne 0) { throw "git archive failed" }
    & $tar -xf "$other/recipe.tar" -C $other
    if ($LASTEXITCODE -ne 0) { throw "tar failed" }
    $tag = Get-Tag $other
    Remove-Item -Recurse -Force $other
    $manifest = Get-Manifest $tag
    if (-not $manifest) { throw "ghcr.io/${repository}:$tag, the one $Fallback's recipe names, isn't published either" }
}
Write-Host "ghcr.io/${repository}:$tag"

# One image of an image index: this platform's, or the only one, checked
# against the digest the index gives it.
$image = $manifest.Read
if ($image.manifests) {
    $images = @($image.manifests | Where-Object { -not $_.platform -or $_.platform.architecture -eq $Arch })
    if ($images.Count -eq 0) { throw "no $Arch image of ghcr.io/${repository}:$tag" }
    $digest = $images[0].digest
    $manifest = Get-Manifest $digest
    if (-not $manifest) { throw "ghcr.io/${repository}@$digest is gone" }
    $sha = [System.Security.Cryptography.SHA256]::Create()
    $hash = "sha256:" + (($sha.ComputeHash($manifest.Bytes) | ForEach-Object { $_.ToString("x2") }) -join "")
    if ($hash -ne $digest) { throw "the $Arch image's manifest is $hash, not $digest" }
    $image = $manifest.Read
}
if (-not $image.layers) { throw "ghcr.io/${repository}:$tag has no files" }

New-Item -ItemType Directory -Force $Destination | Out-Null
$Destination = (Resolve-Path $Destination).Path
foreach ($layer in $image.layers) {
    if ($layer.mediaType -notmatch "tar[.+]gzip$") { throw "a layer of $($layer.mediaType), not a gzipped tar" }
    # The blob is served from elsewhere, to which the token doesn't go.
    $file = Join-Path $Destination "layer.tar.gz"
    Invoke-Retried {
        Invoke-WebRequest -UseBasicParsing "https://ghcr.io/v2/$repository/blobs/$($layer.digest)" `
            -Headers @{ Authorization = "Bearer $token" } -OutFile $file
    } | Out-Null
    $hash = "sha256:" + (Get-FileHash -Algorithm SHA256 $file).Hash.ToLowerInvariant()
    if ($hash -ne $layer.digest) { throw "a layer is $hash, not $($layer.digest)" }
    & $tar -xzf $file -C $Destination
    if ($LASTEXITCODE -ne 0) { throw "tar failed" }
    Remove-Item $file
}

$expected = if ($Platform -eq "windows") { @("bin/ffmpeg.exe", "bin/ffprobe.exe") } else { @("usr/local/bin/ffmpeg", "usr/local/bin/ffprobe", "packages") }
foreach ($path in $expected) {
    if (-not (Test-Path (Join-Path $Destination $path))) { throw "the image has no $path" }
}
