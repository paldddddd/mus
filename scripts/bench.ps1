param(
    [string]$Mode = "image", # image, video, selftest
    [string]$FilePath = "test_wallpaper.bmp",
    [int]$Duration = 10,
    [string]$ExePath = "target\release\wallmus.exe"
)

Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "         WALLMUS PERFORMANCE BENCHMARK (60s / $Duration s)   " -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "Executable : $ExePath"
Write-Host "Mode       : $Mode"
Write-Host "Target     : $FilePath"
Write-Host "Duration   : $Duration seconds"

$args = switch ($Mode) {
    "image"    { @("--image", $FilePath) }
    "video"    { @("--video", $FilePath) }
    "selftest" { @("--selftest") }
    default    { @("--selftest") }
}

$proc = Start-Process -FilePath $ExePath -ArgumentList $args -PassThru

if (-not $proc -or $proc.HasExited) {
    Write-Error "Failed to launch $ExePath"
    exit 1
}

$processId = $proc.Id
Write-Host "Process started with PID $processId. Collecting samples..." -ForegroundColor Yellow

$samples = @()
$startTime = Get-Date

while (-not $proc.HasExited -and ((Get-Date) - $startTime).TotalSeconds -lt $Duration) {
    Start-Sleep -Milliseconds 1000
    try {
        $p = Get-Process -Id $processId -ErrorAction Stop
        $wsMB = [Math]::Round($p.WorkingSet64 / 1MB, 2)
        $cpuSec = $p.CPU
        
        # GPU utilization via nvidia-smi if available
        $gpuUtil = 0
        try {
            $smi = nvidia-smi --query-gpu=utilization.gpu --format=csv,noheader,nounits 2>$null
            if ($smi) {
                $gpuUtil = [double]$smi.Trim()
            }
        } catch {}

        $samples += [PSCustomObject]@{
            Time     = [Math]::Round(((Get-Date) - $startTime).TotalSeconds, 1)
            RSS_MB   = $wsMB
            CPUTime  = $cpuSec
            GPU_Util = $gpuUtil
        }
    } catch {
        break
    }
}

if (-not $proc.HasExited) {
    Stop-Process -Id $processId -Force -ErrorAction SilentlyContinue
}

if ($samples.Count -gt 1) {
    $first = $samples[0]
    $last = $samples[-1]
    $totalTime = $last.Time - $first.Time
    $cpuUsed = $last.CPUTime - $first.CPUTime
    $avgCpuPercent = if ($totalTime -gt 0) { [Math]::Round(($cpuUsed / $totalTime) * 100 / [Environment]::ProcessorCount, 2) } else { 0 }
    
    $avgRss = [Math]::Round(($samples | Measure-Object -Property RSS_MB -Average).Average, 2)
    $peakRss = ($samples | Measure-Object -Property RSS_MB -Maximum).Maximum
    $avgGpu = [Math]::Round(($samples | Measure-Object -Property GPU_Util -Average).Average, 2)
    $peakGpu = ($samples | Measure-Object -Property GPU_Util -Maximum).Maximum

    Write-Host "`n--- BENCHMARK RESULTS ---" -ForegroundColor Green
    Write-Host "Avg CPU Usage : $avgCpuPercent % (system-wide normalized)"
    Write-Host "Avg RAM (RSS) : $avgRss MB"
    Write-Host "Peak RAM(RSS) : $peakRss MB"
    Write-Host "Avg GPU Util  : $avgGpu %"
    Write-Host "Peak GPU Util : $peakGpu %"
    Write-Host "-------------------------" -ForegroundColor Green

    # Output JSON summary
    $summary = @{
        mode = $Mode
        duration_sec = $Duration
        avg_cpu_percent = $avgCpuPercent
        avg_rss_mb = $avgRss
        peak_rss_mb = $peakRss
        avg_gpu_percent = $avgGpu
        peak_gpu_percent = $peakGpu
    } | ConvertTo-Json
    $summary | Out-File -FilePath "bench_results_${Mode}.json" -Encoding utf8
    Write-Host "Saved summary to bench_results_${Mode}.json"
} else {
    Write-Warning "Not enough samples collected."
}
