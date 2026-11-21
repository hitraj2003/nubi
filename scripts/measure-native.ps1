param(
    [string]$Executable = "target/native/release/nubi.exe",
    [string]$Output = "target/native-measurement.json",
    [switch]$Panel
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$binary = (Resolve-Path -LiteralPath (Join-Path $root $Executable)).Path
$data = Join-Path $root "target/native-measure-dev"
New-Item -ItemType Directory -Force -Path $data | Out-Null
$start = New-Object System.Diagnostics.ProcessStartInfo
$start.FileName = $binary
$start.Arguments = if ($Panel) { "--dev --settings --exit-after=80" } else { "--dev --exit-after=80" }
$start.WorkingDirectory = $root
$start.UseShellExecute = $false
$start.CreateNoWindow = $true
$start.WindowStyle = [System.Diagnostics.ProcessWindowStyle]::Hidden
$start.EnvironmentVariables["NUBI_DEV_DATA_DIR"] = $data
$process = [System.Diagnostics.Process]::Start($start)
try {
    for ($second = 0; $second -lt 60; $second++) {
        Start-Sleep -Seconds 1
        if ($process.HasExited) { throw "Native process exited before idle measurement: $($process.ExitCode)" }
    }
    $process.Refresh()
    $cpuBefore = $process.TotalProcessorTime.TotalSeconds
    $clock = [System.Diagnostics.Stopwatch]::StartNew()
    $samples = @()
    for ($sample = 0; $sample -lt 10; $sample++) {
        $process.Refresh()
        $samples += [pscustomobject]@{
            private_bytes = $process.PrivateMemorySize64
            working_set_bytes = $process.WorkingSet64
        }
        Start-Sleep -Seconds 1
        if ($process.HasExited) { throw "Native process exited during idle measurement" }
    }
    $process.Refresh()
    $privateMaximum = ($samples | Measure-Object -Property private_bytes -Maximum).Maximum
    $result = [ordered]@{
        executable = $binary
        development_data = $data
        executable_bytes = (Get-Item -LiteralPath $binary).Length
        executable_sha256 = (Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash
        measured_utc = [DateTime]::UtcNow.ToString("o")
        mode = if ($Panel) { "panel" } else { "idle" }
        warmup_seconds = 60
        sample_seconds = $clock.Elapsed.TotalSeconds
        samples = $samples
        private_max_bytes = $privateMaximum
        private_limit_bytes = if ($Panel) { 120000000 } else { 100000000 }
        private_limit_pass = $privateMaximum -le $(if ($Panel) { 120000000 } else { 100000000 })
        cpu_machine_percent = 100 * ($process.TotalProcessorTime.TotalSeconds - $cpuBefore) / $clock.Elapsed.TotalSeconds / [Environment]::ProcessorCount
    }
    $json = $result | ConvertTo-Json -Depth 5
    [System.IO.File]::WriteAllText((Join-Path $root $Output), $json)
    Write-Output $json
}
finally {
    if (!$process.HasExited) {
        # This process was launched above with development isolation. Never select by name.
        if (!$process.WaitForExit(12000)) { $process.Kill(); $process.WaitForExit() }
    }
    $process.Dispose()
}
