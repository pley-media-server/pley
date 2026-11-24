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
# then, the image master names is taken instead, saying so.

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("windows", "linux")]
    [string]$Platform,
    [Parameter(Mandatory = $true)]
    [string]$Destination,
    [ValidateSet("amd64", "arm64")]
    [string]$Arch = "amd64"
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"
$root = Split-Path $PSScriptRoot -Parent
$onWindows = [System.IO.Path]::DirectorySeparatorChar -eq "\"
$repository = if ($Platform -eq "windows") { "pley-media-server/pley-ffmpeg-windows" } else { "pley-media-server/pley-ffmpeg" }

# Windows' own tar, which reads C:\ paths as paths, not as hosts as Git's does.
$tar = if ($onWindows) { Join-Path $env:SystemRoot "System32\tar.exe" } else { "tar" }

# The image's tag, from tag.sh in the deploy/ffmpeg folder of a checkout.
function Get-Tag([string]$Checkout) {
    $sh = Get-Command sh -ErrorAction SilentlyContinue
    $sh = if ($sh) { $sh.Source } else { Join-Path (Split-Path (Split-Path (Get-Command git).Source)) "bin\sh.exe" }
    $arguments = @("$Checkout/deploy/ffmpeg/tag.sh")
    if ($Platform -eq "windows") { $arguments += "windows" }
    $tag = & $sh @arguments
    if ($LASTEXITCODE -ne 0 -or -not $tag) { throw "tag.sh failed" }
    return "$tag".Trim()
}

$token = (Invoke-RestMethod "https://ghcr.io/token?scope=repository:${repository}:pull").token
$accept = "application/vnd.oci.image.index.v1+json, application/vnd.oci.image.manifest.v1+json, " +
    "application/vnd.docker.distribution.manifest.list.v2+json, application/vnd.docker.distribution.manifest.v2+json"

# A manifest of the image, or $null where there is none (not published).
function Get-Manifest([string]$Reference) {
    try {
        $response = Invoke-WebRequest -UseBasicParsing "https://ghcr.io/v2/$repository/manifests/$Reference" `
            -Headers @{ Authorization = "Bearer $token"; Accept = $accept }
    } catch {
        if ($_.Exception.Response -and [int]$_.Exception.Response.StatusCode -eq 404) { return $null }
        throw
    }
    return [System.Text.Encoding]::UTF8.GetString($response.RawContentStream.ToArray()) | ConvertFrom-Json
}

$tag = Get-Tag $root
$manifest = Get-Manifest $tag
if (-not $manifest) {
    Write-Warning "ghcr.io/${repository}:$tag isn't published yet: taking the one master names"
    $master = Join-Path ([System.IO.Path]::GetTempPath()) "pley-ffmpeg-master-$PID"
    New-Item -ItemType Directory -Force $master | Out-Null
    git -C $root fetch --quiet --depth=1 origin master
    if ($LASTEXITCODE -ne 0) { throw "git fetch failed" }
    git -C $root archive -o "$master/recipe.tar" FETCH_HEAD deploy/ffmpeg
    if ($LASTEXITCODE -ne 0) { throw "git archive failed" }
    & $tar -xf "$master/recipe.tar" -C $master
    if ($LASTEXITCODE -ne 0) { throw "tar failed" }
    $tag = Get-Tag $master
    Remove-Item -Recurse -Force $master
    $manifest = Get-Manifest $tag
    if (-not $manifest) { throw "ghcr.io/${repository}:$tag, the one master names, isn't published either" }
}
Write-Host "ghcr.io/${repository}:$tag"

# One image of an image index: this platform's, or the only one.
if ($manifest.manifests) {
    $images = @($manifest.manifests | Where-Object { -not $_.platform -or $_.platform.architecture -eq $Arch })
    if ($images.Count -eq 0) { throw "no $Arch image of ghcr.io/${repository}:$tag" }
    $manifest = Get-Manifest $images[0].digest
}

New-Item -ItemType Directory -Force $Destination | Out-Null
$Destination = (Resolve-Path $Destination).Path
foreach ($layer in $manifest.layers) {
    if ($layer.mediaType -notmatch "tar[.+]gzip$") { throw "a layer of $($layer.mediaType), not a gzipped tar" }
    # The blob is served from elsewhere, to which the token doesn't go.
    $file = Join-Path $Destination "layer.tar.gz"
    Invoke-WebRequest -UseBasicParsing "https://ghcr.io/v2/$repository/blobs/$($layer.digest)" `
        -Headers @{ Authorization = "Bearer $token" } -OutFile $file
    $hash = "sha256:" + (Get-FileHash -Algorithm SHA256 $file).Hash.ToLowerInvariant()
    if ($hash -ne $layer.digest) { throw "a layer is $hash, not $($layer.digest)" }
    & $tar -xzf $file -C $Destination
    if ($LASTEXITCODE -ne 0) { throw "tar failed" }
    Remove-Item $file
}
