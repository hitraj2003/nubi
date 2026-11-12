[CmdletBinding()]
param([string[]]$Theme = @('dark', 'light'))
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (-not ('ParityExitProbe' -as [type])) {
    Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class ParityExitProbe {
    [DllImport("kernel32.dll", SetLastError=true)]
    public static extern bool GetExitCodeProcess(IntPtr process, out uint code);
}
'@
}
Push-Location $PSScriptRoot
try {
    $results = @()
    foreach ($mode in $Theme) {
        foreach ($view in @('closed', 'open', 'usage', 'general')) {
            $notch = $view -in @('closed', 'open')
            $exe = Join-Path $PSScriptRoot $(if ($notch) { 'target/release/notch-slint.exe' } else { 'target/release/parity.exe' })
            $arguments = @("--theme=$mode", '--exit-after=16')
            if ($notch) { $arguments += "--state=$view" } else { $arguments += "--page=$view" }
            New-Item -ItemType Directory -Force measurements | Out-Null
            $stderr = Join-Path $PSScriptRoot "measurements/parity-$view-$mode.stderr.txt"
            $process = Start-Process -FilePath $exe -ArgumentList $arguments -WindowStyle Hidden -RedirectStandardError $stderr -PassThru
            $processHandle = $process.Handle
            try {
                Start-Sleep -Seconds 3
                $process.Refresh()
                if ($process.HasExited) { throw "Exited before measurement: $view/$mode" }
                $cpuStart = $process.TotalProcessorTime.TotalSeconds
                $clock = [Diagnostics.Stopwatch]::StartNew()
                $samples = @()
                do {
                    $process.Refresh()
                    if ($process.HasExited) { throw "Exited during measurement: $view/$mode" }
                    $samples += $process.PrivateMemorySize64
                    Start-Sleep -Milliseconds 250
                } while ($clock.Elapsed.TotalSeconds -lt 10)
                $process.Refresh()
                $seconds = $clock.Elapsed.TotalSeconds
                $cpu = ($process.TotalProcessorTime.TotalSeconds - $cpuStart) / $seconds * 100
                $row = [pscustomobject]@{
                    Theme = $mode; Surface = $view; Renderer = 'software';
                    ExeBytes = (Get-Item -LiteralPath $exe).Length;
                    Sha256 = (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash;
                    SampleSeconds = [math]::Round($seconds, 3); Samples = $samples.Count;
                    AveragePrivateBytes = [math]::Round(($samples | Measure-Object -Average).Average);
                    MaximumPrivateBytes = ($samples | Measure-Object -Maximum).Maximum;
                    CpuPercentOneCore = [math]::Round($cpu, 4)
                }
                $process.WaitForExit()
                [uint32]$exitCode = 0
                if (-not [ParityExitProbe]::GetExitCodeProcess($processHandle, [ref]$exitCode)) { throw 'Cannot read process exit code' }
                if ($exitCode -ne 0) { throw "Exit ${exitCode}: $view/$mode" }
                $row | Add-Member -NotePropertyName ExitCode -NotePropertyValue $exitCode
                $results += $row
                $results | ConvertTo-Json -Depth 4 | Set-Content -Encoding utf8 measurements/parity.json
                Write-Output "$view/$mode private=$($row.AveragePrivateBytes) B CPU=$cpu % exe=$($row.ExeBytes) B"
            } finally {
                if (-not $process.HasExited) { Stop-Process -Id $process.Id }
                $process.Dispose()
            }
        }
    }
} finally { Pop-Location }
