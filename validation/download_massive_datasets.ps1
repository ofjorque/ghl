# GHL Massive Production Datasets Downloader (PowerShell / Windows)
# Roadmap 05: Higgs Boson (11M) and ASA Airline On-Time (120M / Harvard Dataverse)
param(
    [string]$Target = "full",     # "full" (11M / 2.4M) or "sample" (100k rows)
    [string]$OutputDir = "validation/data"
)

$ErrorActionPreference = "Stop"

Write-Host "============================================================" -ForegroundColor Cyan
Write-Host " GHL Massive Datasets Downloader & Generator (Roadmap 05)" -ForegroundColor Cyan
Write-Host " Target mode: $Target | Output: $OutputDir" -ForegroundColor Yellow
Write-Host "============================================================"

if (!(Test-Path -Path $OutputDir)) {
    New-Item -ItemType Directory -Path $OutputDir -Force | Out-Null
}

$HiggsFile = Join-Path $OutputDir "higgs_11m.csv"
$AirlineFile = Join-Path $OutputDir "airline_120m.csv"
$AirportsFile = Join-Path $OutputDir "airports_master.csv"
$CarriersFile = Join-Path $OutputDir "carriers_master.csv"

# 1. HIGGS BOSON DATASET (UCI ML Repository, 11,000,000 rows x 29 columns)
Write-Host "`n[1/2] Processing Higgs Boson Dataset..." -ForegroundColor Green
if (Test-Path -Path $HiggsFile) {
    $higgsSizeMb = [math]::Round((Get-Item $HiggsFile).Length / 1MB, 1)
    Write-Host "Higgs dataset already exists at $HiggsFile ($higgsSizeMb MB)" -ForegroundColor DarkGray
} else {
    if ($Target -eq "full") {
        Write-Host "Downloading full 11M Higgs dataset from UCI ML Repository (~2.8 GB compressed)..." -ForegroundColor Yellow
        $HiggsGzUrl = "https://archive.ics.uci.edu/ml/machine-learning-databases/00280/HIGGS.csv.gz"
        $GzPath = Join-Path $OutputDir "HIGGS.csv.gz"
        Invoke-WebRequest -Uri $HiggsGzUrl -OutFile $GzPath

        Write-Host "Decompressing HIGGS.csv.gz with official UCI column header..." -ForegroundColor Yellow
        $header = "label,lepton_pt,lepton_eta,lepton_phi,met,missing_energy_phi,jet1_pt,jet1_eta,jet1_phi,jet1_b_tag,jet2_pt,jet2_eta,jet2_phi,jet2_b_tag,jet3_pt,jet3_eta,jet3_phi,jet3_b_tag,jet4_pt,jet4_eta,jet4_phi,jet4_b_tag,m_jj,m_jjj,m_lv,m_jlv,m_bb,m_wbb,m_wwbb`n"
        $outStream = [System.IO.File]::Create($HiggsFile, 1048576)
        $headerBytes = [System.Text.Encoding]::ASCII.GetBytes($header)
        $outStream.Write($headerBytes, 0, $headerBytes.Length)

        $inStream = [System.IO.File]::OpenRead($GzPath)
        $gzStream = New-Object System.IO.Compression.GZipStream($inStream, [System.IO.Compression.CompressionMode]::Decompress)
        $buffer = New-Object byte[] 1048576
        while (($read = $gzStream.Read($buffer, 0, $buffer.Length)) -gt 0) {
            $outStream.Write($buffer, 0, $read)
        }
        $outStream.Flush()
        $outStream.Close()
        $gzStream.Close()
        $inStream.Close()
        Remove-Item -Path $GzPath -Force
    } else {
        Write-Host "Generating representative 100,000-row Higgs kinematics sample dataset..." -ForegroundColor Cyan
        $inv = [System.Globalization.CultureInfo]::InvariantCulture
        $Sb = [System.Text.StringBuilder]::new()
        [void]$Sb.AppendLine("label,lepton_pt,met,m_jj,lepton_eta")
        $Rng = [System.Random]::new(42)
        for ($i = 0; $i -lt 100000; $i++) {
            $lbl = if ($Rng.NextDouble() -gt 0.5) { 1.0 } else { 0.0 }
            $pt = 60.0 + $Rng.NextDouble() * 50.0 + ($lbl * 15.0)
            $m = 30.0 + $Rng.NextDouble() * 40.0 + ($lbl * 10.0)
            $mjj = 100.0 + $Rng.NextDouble() * 40.0 + ($lbl * 12.0)
            $eta = ($Rng.NextDouble() * 4.0) - 2.0
            $line = $lbl.ToString("F1", $inv) + "," + $pt.ToString("F2", $inv) + "," + $m.ToString("F2", $inv) + "," + $mjj.ToString("F2", $inv) + "," + $eta.ToString("F3", $inv)
            [void]$Sb.AppendLine($line)
        }
        [System.IO.File]::WriteAllText($HiggsFile, $Sb.ToString())
    }
    Write-Host "Higgs dataset ready: $HiggsFile" -ForegroundColor Green
}

# 2. AIRLINE ON-TIME PERFORMANCE DATASET (Harvard Dataverse / ASA Data Expo)
Write-Host "`n[2/2] Processing Airline On-Time Performance Dataset..." -ForegroundColor Green
if (Test-Path -Path $AirlineFile) {
    $airlineSizeMb = [math]::Round((Get-Item $AirlineFile).Length / 1MB, 1)
    Write-Host "Airline dataset already exists at $AirlineFile ($airlineSizeMb MB)" -ForegroundColor DarkGray
} else {
    if ($Target -eq "full") {
        Write-Host "Downloading real Harvard Dataverse master catalogs..." -ForegroundColor Yellow
        Invoke-WebRequest -Uri "https://dataverse.harvard.edu/api/access/datafile/1374930" -OutFile $AirportsFile
        Invoke-WebRequest -Uri "https://dataverse.harvard.edu/api/access/datafile/1374931" -OutFile $CarriersFile

        Write-Host "Downloading ASA Data Expo real flight records (2008, ~39 MB compressed)..." -ForegroundColor Yellow
        $Bz2Path = Join-Path $OutputDir "2008.csv.bz2"
        Invoke-WebRequest -Uri "https://dataverse.harvard.edu/api/access/datafile/1374917" -OutFile $Bz2Path

        Write-Host "Decompressing 2008.csv.bz2..." -ForegroundColor Yellow
        python -c "import bz2, shutil; (shutil.copyfileobj(bz2.open('$($Bz2Path.Replace('\', '/'))', 'rb'), open('$($AirlineFile.Replace('\', '/'))', 'wb')))"
        Remove-Item -Path $Bz2Path -Force
    } else {
        Write-Host "Generating representative 100,000-row Airline On-Time multi-carrier sample dataset..." -ForegroundColor Cyan
        $inv = [System.Globalization.CultureInfo]::InvariantCulture
        $Carriers = @("WN", "AA", "DL", "UA", "B6", "AS", "NK")
        $Airports = @("ATL", "ORD", "DFW", "DEN", "CLT", "LAX", "IAH", "PHX", "MCO", "SEA")
        $Sb = [System.Text.StringBuilder]::new()
        [void]$Sb.AppendLine("carrier,origin,dest,dep_delay,arr_delay,air_time,distance")
        $Rng = [System.Random]::new(12345)
        for ($i = 0; $i -lt 100000; $i++) {
            $c = $Carriers[$Rng.Next($Carriers.Length)]
            $orig = $Airports[$Rng.Next($Airports.Length)]
            $dest = $Airports[$Rng.Next($Airports.Length)]
            while ($dest -eq $orig) {
                $dest = $Airports[$Rng.Next($Airports.Length)]
            }
            $dep = [Math]::Round(($Rng.NextDouble() * 60.0) - 10.0, 1)
            $arr = [Math]::Round($dep + ($Rng.NextDouble() * 20.0) - 5.0, 1)
            $dist = [Math]::Round(300.0 + ($Rng.NextDouble() * 2200.0), 0)
            $time = [Math]::Round($dist / 8.0 + ($Rng.NextDouble() * 20.0), 1)
            $line = $c + "," + $orig + "," + $dest + "," + $dep.ToString("F1", $inv) + "," + $arr.ToString("F1", $inv) + "," + $time.ToString("F1", $inv) + "," + $dist.ToString("F0", $inv)
            [void]$Sb.AppendLine($line)
        }
        [System.IO.File]::WriteAllText($AirlineFile, $Sb.ToString())
    }
    Write-Host "Airline dataset ready: $AirlineFile" -ForegroundColor Green
}

Write-Host "`n(U・ᴥ・U) All validation datasets successfully initialized in $OutputDir!" -ForegroundColor Green
