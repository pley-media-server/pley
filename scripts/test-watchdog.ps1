# Runs a test command (`cargo test …`) and ends any test program that runs
# for longer than -Minutes, after taking what's needed to tell where it was
# stuck. Without it, a test that never finishes keeps CI's job until its
# time limit, and the log ends at "running N tests" without naming the test.
#
#   scripts/test-watchdog.ps1 [-Minutes 5] [-Dumps test-dumps] -- cargo test --workspace --locked
#
# For a stuck test program (a file in target\debug\deps), the watchdog
#  * prints the stacks of its threads, when cdb (Debugging Tools for Windows)
#    is installed; libtest's threads carry their test's name;
#  * writes a minidump of it to -Dumps, with its .exe and .pdb beside it, to
#    open in WinDbg;
#  * ends it: cargo test then fails, naming the program.
# The exit code is the command's. Windows only.

[CmdletBinding(PositionalBinding = $false)]
param(
    [double]$Minutes = 5,
    [string]$Dumps = "test-dumps",
    # The folder of the test programs; target\debug\deps by default.
    [string]$Deps,
    # The process ID of the run to watch, for the watchdog's own process.
    [int]$Watch = 0,
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$Command
)

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent

if (-not $Deps) {
    $target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $root "target" }
    $Deps = Join-Path $target "debug\deps"
}
$Deps = [IO.Path]::GetFullPath($Deps).TrimEnd('\') + '\'
if (-not [IO.Path]::IsPathRooted($Dumps)) { $Dumps = Join-Path (Get-Location) $Dumps }
$Dumps = [IO.Path]::GetFullPath($Dumps)

function Find-Cdb {
    $found = Get-Command cdb.exe -ErrorAction SilentlyContinue
    if ($found) { return $found.Source }
    foreach ($kits in @(${env:ProgramFiles(x86)}, $env:ProgramFiles)) {
        if (-not $kits) { continue }
        $cdb = Join-Path $kits "Windows Kits\10\Debuggers\x64\cdb.exe"
        if (Test-Path $cdb) { return $cdb }
    }
    return $null
}

function Save-Minidump($process, [string]$file) {
    if (-not ("PleyWatchdog.Dump" -as [type])) {
        Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;
namespace PleyWatchdog {
    public static class Dump {
        [DllImport("dbghelp.dll", SetLastError = true)]
        public static extern bool MiniDumpWriteDump(IntPtr process, uint processId, SafeFileHandle file,
            uint type, IntPtr exception, IntPtr userStream, IntPtr callback);
    }
}
"@
    }
    # Threads, their stacks and what they point to, handles and memory
    # regions: enough to see what each thread waits for, without the whole
    # heap.
    $type = 0x1000 -bor 0x800 -bor 0x40 -bor 0x20 -bor 0x4
    $stream = [IO.File]::Create($file)
    try {
        $ok = [PleyWatchdog.Dump]::MiniDumpWriteDump($process.Handle, [uint32]$process.Id, $stream.SafeFileHandle,
            [uint32]$type, [IntPtr]::Zero, [IntPtr]::Zero, [IntPtr]::Zero)
    } finally {
        $stream.Dispose()
    }
    if (-not $ok) { throw "MiniDumpWriteDump failed: $([Runtime.InteropServices.Marshal]::GetLastWin32Error())" }
}

function Stop-StuckTest($process, [string]$path) {
    $name = [IO.Path]::GetFileNameWithoutExtension($path)
    $ran = [int]((Get-Date) - $process.StartTime).TotalMinutes
    Write-Host "::error::$name has run for $ran minutes: a test in it is stuck. Its threads follow, then it is ended."
    $cdb = Find-Cdb
    if ($cdb) {
        # Noninvasive: reads the threads without becoming the debugger.
        & $cdb -pv -p $process.Id -y $Deps -lines -c "~; ~*k 80; q" | Out-Host
    } else {
        Write-Host "No cdb here to print the threads: they are in the minidump in $Dumps."
    }
    try {
        New-Item -ItemType Directory -Force $Dumps | Out-Null
        Save-Minidump $process (Join-Path $Dumps "$name.dmp")
        Copy-Item $path $Dumps
        $pdb = [IO.Path]::ChangeExtension($path, ".pdb")
        if (Test-Path $pdb) { Copy-Item $pdb $Dumps }
        Write-Host "Saved a minidump of $name, with its program and symbols, in $Dumps."
    } catch {
        Write-Host "Could not save a minidump of ${name}: $_"
    }
    Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
}

if ($Watch) {
    $ended = @{}
    while (Get-Process -Id $Watch -ErrorAction SilentlyContinue) {
        Start-Sleep -Seconds 5
        foreach ($process in Get-Process) {
            if ($ended.ContainsKey($process.Id)) { continue }
            try {
                $path = $process.Path
                $started = $process.StartTime
            } catch {
                continue
            }
            if (-not $path -or -not $path.StartsWith($Deps, [StringComparison]::OrdinalIgnoreCase)) { continue }
            if (((Get-Date) - $started).TotalMinutes -lt $Minutes) { continue }
            $ended[$process.Id] = $true
            Stop-StuckTest $process $path
        }
    }
    exit 0
}

if (-not $Command) { throw "no command to run: scripts/test-watchdog.ps1 -- cargo test …" }

$cdb = Find-Cdb
$how = if ($cdb) { "prints their threads with $cdb" } else { "keeps a minidump of them (no cdb here)" }
Write-Host "Test programs in $Deps that run for over $Minutes minutes are ended; the watchdog $how."
$shell = (Get-Process -Id $PID).Path
$arguments = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "`"$PSCommandPath`"", "-Watch", $PID,
    "-Minutes", $Minutes.ToString([Globalization.CultureInfo]::InvariantCulture), "-Dumps", "`"$Dumps`"", "-Deps", "`"$($Deps.TrimEnd('\'))`"")
$watchdog = Start-Process -FilePath $shell -ArgumentList $arguments -NoNewWindow -PassThru
try {
    $program = $Command[0]
    $rest = @($Command | Select-Object -Skip 1)
    & $program @rest
    $code = $LASTEXITCODE
} finally {
    Stop-Process -Id $watchdog.Id -Force -ErrorAction SilentlyContinue
}
exit $code
