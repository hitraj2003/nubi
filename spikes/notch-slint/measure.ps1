[CmdletBinding()]
param(
    [ValidateSet('femtovg', 'software')]
    [string[]]$Renderer = @('femtovg', 'software'),
    [ValidateSet('dark', 'light')]
    [string]$Theme = 'dark',
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (-not ('NotchSlintWindowProbe' -as [type])) {
    Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class NotchSlintWindowProbe {
    [DllImport("user32.dll")] public static extern IntPtr GetWindowLongPtrW(IntPtr window, int index);
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr window);
}
'@
}
Push-Location $PSScriptRoot
try {
    New-Item -ItemType Directory -Force measurements | Out-Null
    $logicalProcessors = [Environment]::ProcessorCount
    $buildTarget = Join-Path $PSScriptRoot 'target'
    foreach ($engine in $Renderer) {
        $target = Join-Path $PSScriptRoot "target/$engine"
        $exe = Join-Path $target 'release/notch-slint.exe'
        if (-not $SkipBuild) {
            # Reuse dependency artifacts, but retain each renderer's exact executable.
            & cargo build --release --offline --no-default-features --features $engine --target-dir $buildTarget
            if ($LASTEXITCODE -ne 0) { throw "Release build failed: $engine" }
            & cargo clippy --release --offline --no-default-features --features $engine --target-dir $buildTarget -- -D warnings
            if ($LASTEXITCODE -ne 0) { throw "Clippy failed: $engine" }
            New-Item -ItemType Directory -Force (Split-Path -Parent $exe) | Out-Null
            Copy-Item -LiteralPath (Join-Path $buildTarget 'release/notch-slint.exe') -Destination $exe
        }
        $exeBytes = (Get-Item -LiteralPath $exe).Length
        $sha256 = (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash
        foreach ($state in @('closed', 'open')) {
            # The visible notch is the surface being measured. Do not hide its window.
            $process = Start-Process -FilePath $exe -ArgumentList @("--renderer=$engine", "--theme=$Theme", "--state=$state", '--exit-after=30') -PassThru
            try {
                Start-Sleep -Seconds 3
                $process.Refresh()
                if ($process.HasExited) { throw "$engine exited before sampling; code $($process.ExitCode)" }
                $window = $process.MainWindowHandle
                if ($window -eq [IntPtr]::Zero) { throw "$engine has no visible native window" }
                $style = [NotchSlintWindowProbe]::GetWindowLongPtrW($window, -20).ToInt64()
                if (($style -band 0x08000008) -ne 0x08000008) { throw "$engine native window is not ready/topmost/no-activate" }
                $windowDpi = [NotchSlintWindowProbe]::GetDpiForWindow($window)
                $cpuStart = $process.TotalProcessorTime.TotalSeconds
                $clock = [Diagnostics.Stopwatch]::StartNew()
                $samples = @()
                $previousSampleSeconds = 0.0
                $maxSampleGapSeconds = 0.0
                do {
                    $process.Refresh()
                    if ($process.HasExited) { throw "$engine exited during sampling" }
                    $sampleSeconds = $clock.Elapsed.TotalSeconds
                    $maxSampleGapSeconds = [math]::Max($maxSampleGapSeconds, $sampleSeconds - $previousSampleSeconds)
                    $previousSampleSeconds = $sampleSeconds
                    $samples += [pscustomobject]@{
                        Seconds = $sampleSeconds
                        PrivateBytes = $process.PrivateMemorySize64
                        WorkingSetBytes = $process.WorkingSet64
                        CpuSeconds = $process.TotalProcessorTime.TotalSeconds
                    }
                    Start-Sleep -Milliseconds 100
                } while ($clock.Elapsed.TotalSeconds -lt 10)
                $process.Refresh()
                $cpuSeconds = $process.TotalProcessorTime.TotalSeconds - $cpuStart
                $elapsed = $clock.Elapsed.TotalSeconds
                $clock.Stop()
                $maxSampleGapSeconds = [math]::Max($maxSampleGapSeconds, $elapsed - $previousSampleSeconds)
                if ($elapsed -gt 11 -or $maxSampleGapSeconds -gt 0.5) {
                    throw "Interrupted $engine/$state sample (${elapsed}s, max gap ${maxSampleGapSeconds}s); rerun with -SkipBuild."
                }
                $private = $samples.PrivateBytes | Measure-Object -Average -Minimum -Maximum
                $row = [pscustomobject]@{
                    Renderer = $engine
                    State = $state
                    Theme = $Theme
                    ExeBytes = $exeBytes
                    ExeSha256 = $sha256
                    SampleSeconds = $elapsed
                    WarmupSeconds = 3
                    SampleCount = $samples.Count
                    MaxSampleGapSeconds = $maxSampleGapSeconds
                    LogicalProcessors = $logicalProcessors
                    WindowDpi = $windowDpi
                    PrivateBytesMean = [math]::Round($private.Average)
                    PrivateBytesMin = $private.Minimum
                    PrivateBytesMax = $private.Maximum
                    WorkingSetBytesMean = [math]::Round(($samples.WorkingSetBytes | Measure-Object -Average).Average)
                    CpuSeconds = $cpuSeconds
                    CpuMachinePercent = 100 * $cpuSeconds / $elapsed / $logicalProcessors
                    CpuOneCorePercent = 100 * $cpuSeconds / $elapsed
                    RecordedUtc = [DateTime]::UtcNow.ToString('o')
                    WindowsVersion = [Environment]::OSVersion.VersionString
                }
                $samples | Export-Csv -NoTypeInformation -LiteralPath "measurements/$engine-$state-$Theme-samples.csv"
                $row | ConvertTo-Json | Set-Content -LiteralPath "measurements/$engine-$state-$Theme.json" -Encoding UTF8
                $row | Format-List
            } finally {
                if (-not $process.HasExited) { $process.Kill(); $process.WaitForExit() }
                $process.Dispose()
            }
        }
    }
    # Preserve the other renderer's accepted samples when repeating only one.
    $results = foreach ($savedEngine in @('femtovg', 'software')) {
        foreach ($savedState in @('closed', 'open')) {
            $savedPath = "measurements/$savedEngine-$savedState-$Theme.json"
            if (Test-Path -LiteralPath $savedPath) {
                Get-Content -Raw -LiteralPath $savedPath | ConvertFrom-Json
            }
        }
    }
    $results | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath "measurements/summary-$Theme.json" -Encoding UTF8
} finally {
    Pop-Location
}
