[CmdletBinding()]
param(
    [ValidateSet('dark', 'light')]
    [string[]]$Theme = @('dark', 'light'),
    [ValidateSet('panel', 'approval', 'about')]
    [string[]]$Surface = @('panel', 'approval'),
    [switch]$InspectAccessibility,
    [switch]$ExerciseControls,
    [ValidateRange(1, 50)]
    [int]$CycleCount = 5
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (-not ('H10Probe' -as [type])) {
    Add-Type @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class H10Probe {
    private delegate bool EnumProc(IntPtr window, IntPtr data);
    [DllImport("user32.dll")] private static extern bool EnumWindows(EnumProc callback, IntPtr data);
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
    [DllImport("user32.dll")] private static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] private static extern int GetWindowTextW(IntPtr window, StringBuilder text, int count);
    [DllImport("user32.dll")] public static extern IntPtr GetWindowLongPtrW(IntPtr window, int index);
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern IntPtr SendMessageW(IntPtr window, uint message, IntPtr wparam, IntPtr lparam);
    public static IntPtr[] Windows(int pid) {
        var found = new List<IntPtr>();
        EnumWindows((window, data) => {
            uint owner; GetWindowThreadProcessId(window, out owner);
            if (owner == pid && IsWindowVisible(window)) found.Add(window);
            return true;
        }, IntPtr.Zero);
        return found.ToArray();
    }
    public static string Title(IntPtr window) {
        var text = new StringBuilder(512); GetWindowTextW(window, text, 512); return text.ToString();
    }
}
'@
}

Push-Location $PSScriptRoot
try {
    $exe = Join-Path $PSScriptRoot 'target/release/toolkit-gate.exe'
    $exeBytes = (Get-Item -LiteralPath $exe).Length
    $sha256 = (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash
    New-Item -ItemType Directory -Force measurements | Out-Null
    foreach ($mode in $Theme) {
        foreach ($view in $Surface) {
            $stderr = Join-Path $PSScriptRoot "measurements/h10-$view-$mode.stderr.txt"
            $lifetime = if ($ExerciseControls) { 30 + 30 * $CycleCount } else { 45 }
            $process = Start-Process -FilePath $exe -ArgumentList @("--surface=$view", "--theme=$mode", "--exit-after=$lifetime") -WindowStyle Hidden -RedirectStandardError $stderr -PassThru
            try {
                Start-Sleep -Seconds 3
                $process.Refresh()
                if ($process.HasExited) { throw "Gate exited before sampling: $($process.ExitCode). See $stderr" }
                $windows = @(foreach ($handle in [H10Probe]::Windows($process.Id)) {
                    $style = [H10Probe]::GetWindowLongPtrW($handle, -20).ToInt64()
                    [pscustomobject]@{
                        Handle = $handle.ToInt64()
                        Title = [H10Probe]::Title($handle)
                        ExStyle = ('0x{0:X}' -f $style)
                        NoActivate = ($style -band 0x08000000) -ne 0
                        Topmost = ($style -band 0x8) -ne 0
                        Dpi = [H10Probe]::GetDpiForWindow($handle)
                    }
                })
                $notch = @($windows | Where-Object { $_.Title -like '*Slint spike*' })
                $active = @($windows | Where-Object { $_.Title -like 'Nubi H10*' })
                if ($notch.Count -ne 1 -or $active.Count -ne 1) { throw 'Expected one visible notch and one interactive surface' }
                if (-not $notch[0].NoActivate -or -not $notch[0].Topmost) { throw 'Notch styles incorrect' }
                if ($active[0].NoActivate) { throw 'Interactive surface unexpectedly no-activate' }
                $foreground = [H10Probe]::GetForegroundWindow().ToInt64()
                $mouseActivate = [H10Probe]::SendMessageW([IntPtr]$notch[0].Handle, 0x21, [IntPtr]::Zero, [IntPtr]::Zero).ToInt64()
                if ($mouseActivate -ne 3) { throw 'Notch does not return MA_NOACTIVATE' }
                $cpuStart = $process.TotalProcessorTime.TotalSeconds
                $clock = [Diagnostics.Stopwatch]::StartNew()
                $samples = @()
                $previous = 0.0
                $maxGap = 0.0
                do {
                    $process.Refresh()
                    if ($process.HasExited) { throw 'Gate exited during sampling' }
                    $seconds = $clock.Elapsed.TotalSeconds
                    $maxGap = [math]::Max($maxGap, $seconds - $previous)
                    $previous = $seconds
                    $samples += [pscustomobject]@{
                        Seconds = $seconds
                        PrivateBytes = $process.PrivateMemorySize64
                        WorkingSetBytes = $process.WorkingSet64
                        CpuSeconds = $process.TotalProcessorTime.TotalSeconds
                    }
                    Start-Sleep -Milliseconds 100
                } while ($clock.Elapsed.TotalSeconds -lt 10)
                $process.Refresh()
                $elapsed = $clock.Elapsed.TotalSeconds
                $cpu = $process.TotalProcessorTime.TotalSeconds - $cpuStart
                $maxGap = [math]::Max($maxGap, $elapsed - $previous)
                if ($elapsed -gt 11 -or $maxGap -gt 0.5) { throw "Interrupted sample: ${elapsed}s, largest gap ${maxGap}s" }
                $memory = $samples.PrivateBytes | Measure-Object -Average -Minimum -Maximum
                $row = [pscustomobject]@{
                    Surface = $view
                    Theme = $mode
                    Renderer = 'software (BackendSelector, no environment fallback)'
                    ExeBytes = $exeBytes
                    ExeSha256 = $sha256
                    IncludesPassiveNotch = $true
                    PrivateBytesMean = [math]::Round($memory.Average)
                    PrivateBytesMin = $memory.Minimum
                    PrivateBytesMax = $memory.Maximum
                    PrivateLimitBytes = 120000000
                    PrivatePass = $memory.Maximum -le 120000000
                    CpuSeconds = $cpu
                    CpuMachinePercent = 100 * $cpu / $elapsed / [Environment]::ProcessorCount
                    CpuOneCorePercent = 100 * $cpu / $elapsed
                    LogicalProcessors = [Environment]::ProcessorCount
                    WarmupSeconds = 3
                    SampleSeconds = $elapsed
                    SampleCount = $samples.Count
                    MaxSampleGapSeconds = $maxGap
                    ForegroundHandle = $foreground
                    ForegroundIsInteractiveSurface = $foreground -eq $active[0].Handle
                    NotchMouseActivateResult = $mouseActivate
                    Windows = $windows
                    RecordedUtc = [DateTime]::UtcNow.ToString('o')
                    WindowsVersion = [Environment]::OSVersion.VersionString
                }
                $samples | Export-Csv -NoTypeInformation -LiteralPath "measurements/h10-$view-$mode-samples.csv"
                $row | ConvertTo-Json -Depth 5 | Set-Content -Encoding UTF8 -LiteralPath "measurements/h10-$view-$mode.json"
                $row | Select-Object Surface, Theme, PrivateBytesMean, PrivateBytesMax, CpuMachinePercent, ForegroundIsInteractiveSurface | Format-Table
                if ($InspectAccessibility -or $ExerciseControls) {
                    # Query after sampling so UI Automation work cannot contaminate idle CPU.
                    try {
                        Add-Type -AssemblyName UIAutomationClient
                        Add-Type -AssemblyName UIAutomationTypes
                        $root = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]$active[0].Handle)
                        $nodes = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
                        $tree = @(foreach ($node in $nodes) {
                            $current = $node.Current
                            $valuePattern = $null
                            $hasValue = $node.TryGetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern, [ref]$valuePattern)
                            [pscustomobject]@{
                                Name = $current.Name
                                Role = $current.ControlType.ProgrammaticName
                                Focusable = $current.IsKeyboardFocusable
                                Focused = $current.HasKeyboardFocus
                                HelpText = $current.HelpText
                                Patterns = @($node.GetSupportedPatterns() | ForEach-Object { $_.ProgrammaticName })
                                Value = $(if ($hasValue) { $valuePattern.Current.Value } else { $null })
                                ReadOnly = $(if ($hasValue) { $valuePattern.Current.IsReadOnly } else { $null })
                            }
                        })
                        $tree | ConvertTo-Json -Depth 4 | Set-Content -Encoding UTF8 -LiteralPath "measurements/h10-$view-$mode-uia.json"
                        if ($ExerciseControls -and $view -eq 'panel') {
                            function Find-Button($automationRoot, [string]$name) {
                                $nameCondition = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, $name)
                                $roleCondition = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::Button)
                                $condition = New-Object System.Windows.Automation.AndCondition($nameCondition, $roleCondition)
                                $button = $automationRoot.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
                                if ($null -eq $button) { throw "UI Automation button missing: $name" }
                                return $button
                            }
                            function Invoke-Button($automationRoot, [string]$name) {
                                $button = Find-Button $automationRoot $name
                                $pattern = $null
                                if ($button.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {
                                    $pattern.Invoke()
                                } elseif ($button.TryGetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern, [ref]$pattern)) {
                                    if ($pattern.Current.ExpandCollapseState -eq [System.Windows.Automation.ExpandCollapseState]::Collapsed) { $pattern.Expand() } else { $pattern.Collapse() }
                                } elseif ($button.TryGetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern, [ref]$pattern)) {
                                    $pattern.Toggle()
                                } else { throw "No action pattern for button: $name" }
                                Start-Sleep -Milliseconds 150
                            }
                            $process.Refresh()
                            $before = $process.PrivateMemorySize64
                            $requestCount = $CycleCount
                            $copied = $false
                            $expanded = 'untested'
                            for ($iteration = 0; $iteration -lt $requestCount; $iteration++) {
                                Invoke-Button $root 'Approval card'
                                $cardHandle = @([H10Probe]::Windows($process.Id) | Where-Object { [H10Probe]::Title($_) -eq 'Nubi H10 approval' })
                                if ($cardHandle.Count -ne 1) { throw 'Approval did not become visible' }
                                $cardRoot = [System.Windows.Automation.AutomationElement]::FromHandle($cardHandle[0])
                                if ($iteration -eq 0) {
                                    $disclosure = Find-Button $cardRoot 'All parameters'
                                    $expandPattern = $null
                                    $hasExpand = $disclosure.TryGetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern, [ref]$expandPattern)
                                    $collapsedState = $(if ($hasExpand) { $expandPattern.Current.ExpandCollapseState.ToString() } else { 'ExpandCollapsePattern unavailable' })
                                    Invoke-Button $cardRoot 'All parameters'
                                    $expanded = $(if ($hasExpand) { "$collapsedState then $($expandPattern.Current.ExpandCollapseState)" } else { $collapsedState })
                                    Invoke-Button $cardRoot 'Copy command'
                                    $copied = (Get-Clipboard -Raw) -eq 'cargo test --offline'
                                }
                                Invoke-Button $cardRoot 'Deny'
                                $visibleCards = @([H10Probe]::Windows($process.Id) | Where-Object { [H10Probe]::Title($_) -eq 'Nubi H10 approval' })
                                if ($visibleCards.Count -ne 0) { throw 'Deny did not hide the approval' }
                                $process.Refresh()
                                [pscustomobject]@{
                                    Cycles = $iteration + 1
                                    ExeSha256 = $sha256
                                    PrivateBytesBefore = $before
                                    PrivateBytesAfter = $process.PrivateMemorySize64
                                    PrivateBytesDelta = $process.PrivateMemorySize64 - $before
                                    NativeClipboardMatchesCommand = $copied
                                    DisclosureStates = $expanded
                                } | ConvertTo-Json | Set-Content -Encoding UTF8 -LiteralPath "measurements/h10-$view-$mode-interaction.json"
                            }
                            Start-Sleep -Milliseconds 500
                            $process.Refresh()
                            [pscustomobject]@{
                                Cycles = $requestCount
                                ExeSha256 = $sha256
                                PrivateBytesBefore = $before
                                PrivateBytesAfter = $process.PrivateMemorySize64
                                PrivateBytesDelta = $process.PrivateMemorySize64 - $before
                                PrivateLimitPass = $process.PrivateMemorySize64 -le 120000000
                                NativeClipboardMatchesCommand = $copied
                                DisclosureStates = $expanded
                                Method = 'UI Automation InvokePattern and ExpandCollapsePattern; no physical keyboard or mouse'
                            } | ConvertTo-Json | Set-Content -Encoding UTF8 -LiteralPath "measurements/h10-$view-$mode-interaction.json"
                        }
                        $oldError = Join-Path $PSScriptRoot "measurements/h10-$view-$mode-uia-error.txt"
                        if (Test-Path -LiteralPath $oldError) { Remove-Item -LiteralPath $oldError }
                    } catch {
                        $process.Refresh()
                        "$($_.Exception.Message)`n$($_.ScriptStackTrace)`nProcess exited: $($process.HasExited)" | Set-Content -LiteralPath "measurements/h10-$view-$mode-uia-error.txt"
                    }
                }
            } finally {
                if (-not $process.HasExited) { $process.Kill(); $process.WaitForExit() }
                $process.Dispose()
            }
        }
    }
} finally {
    Pop-Location
}
