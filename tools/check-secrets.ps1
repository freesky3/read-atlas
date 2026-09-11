param([string]$ReportDirectory = "tmp/opensource/secrets")
$ErrorActionPreference = "Stop"
$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
Push-Location $repoRoot
try {
    $reportRoot = [IO.Path]::GetFullPath((Join-Path $repoRoot $ReportDirectory))
    if (-not $reportRoot.StartsWith($repoRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Reports must stay within the repository."
    }
    if ($env:OS -ne "Windows_NT") { throw "This pinned scanner runner currently supports Windows x64." }
    $version = "8.30.1"
    $toolRoot = Join-Path $repoRoot "tmp/tools/gitleaks-$version"
    New-Item -ItemType Directory -Force $toolRoot, $reportRoot | Out-Null
    $archiveName = "gitleaks_${version}_windows_x64.zip"
    $archive = Join-Path $toolRoot $archiveName
    $base = "https://github.com/gitleaks/gitleaks/releases/download/v$version"
    $checksumFile = Join-Path $toolRoot "checksums.txt"
    Invoke-WebRequest "$base/gitleaks_${version}_checksums.txt" -OutFile $checksumFile -UseBasicParsing
    $checksums = Get-Content -Raw -LiteralPath $checksumFile
    $line = @($checksums -split "\r?\n" | Where-Object { $_ -match [regex]::Escape($archiveName) + '$' })
    if ($line.Count -ne 1) { throw "Missing unique official archive checksum." }
    $expected = ($line[0] -split "\s+")[0]
    if (-not (Test-Path -LiteralPath $archive)) {
        Invoke-WebRequest "$base/$archiveName" -OutFile $archive -UseBasicParsing
    }
    if ((Get-FileHash -Algorithm SHA256 -LiteralPath $archive).Hash.ToLowerInvariant() -ne $expected.ToLowerInvariant()) {
        throw "Gitleaks checksum mismatch. Do not run this archive."
    }
    Expand-Archive -LiteralPath $archive -DestinationPath $toolRoot -Force
    $scanner = Join-Path $toolRoot "gitleaks.exe"
    & $scanner git $repoRoot --log-opts="--all" --redact=100 --no-banner --report-format json --report-path (Join-Path $reportRoot "history.json")
    if ($LASTEXITCODE -ne 0) { throw "History scan failed; inspect the redacted report." }

    # Scan the exact candidate source set, including not-yet-tracked implementation files.
    $snapshot = Join-Path $reportRoot ("candidate-" + [guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Force $snapshot | Out-Null
    $paths = @((git -c core.quotePath=false ls-files --cached --others --exclude-standard) | Sort-Object -Unique)
    if ($LASTEXITCODE -ne 0) { throw "Unable to enumerate the source candidate." }
    foreach ($path in $paths) {
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { continue }
        $source = [IO.Path]::GetFullPath((Join-Path $repoRoot $path))
        $target = [IO.Path]::GetFullPath((Join-Path $snapshot $path))
        if (-not $source.StartsWith($repoRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -or
            -not $target.StartsWith($snapshot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
            throw "Candidate path escaped its root."
        }
        New-Item -ItemType Directory -Force ([IO.Path]::GetDirectoryName($target)) | Out-Null
        Copy-Item -LiteralPath $source -Destination $target
    }
    & $scanner dir $snapshot --redact=100 --no-banner --report-format json --report-path (Join-Path $reportRoot "candidate.json")
    if ($LASTEXITCODE -ne 0) { throw "Candidate scan failed; inspect the redacted report." }
    Write-Output "Gitleaks passed for reachable history and the current source candidate."
} finally {
    Pop-Location
}
