# PowerShell TTFX Benchmark Harness
# Measures process cold-start latency: process startup + parse + check + execute + SVG plot

$ghlExe = ".\target\debug\ghl.exe"
if (Test-Path ".\target\release\ghl.exe") {
    $ghlExe = ".\target\release\ghl.exe"
}

$script = ".\benchmarks\scripts\suite_06_clbg\bench_ttfx.ghl"

Write-Host "=== Benchmarking Time-to-First-Plot (TTFX) ===" -ForegroundColor Cyan
Write-Host "Executable: $ghlExe"
Write-Host "Script: $script"

$times = @()
for ($i = 0; $i -lt 10; $i++) {
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    & $ghlExe run $script *>$null
    $sw.Stop()
    $times += $sw.Elapsed.TotalMilliseconds
}

$avg = ($times | Measure-Object -Average).Average
$min = ($times | Measure-Object -Minimum).Minimum
$max = ($times | Measure-Object -Maximum).Maximum

Write-Host "TTFX Latency over 10 cold runs:" -ForegroundColor Green
Write-Host "  Min:     $([math]::Round($min, 2)) ms"
Write-Host "  Average: $([math]::Round($avg, 2)) ms"
Write-Host "  Max:     $([math]::Round($max, 2)) ms"
