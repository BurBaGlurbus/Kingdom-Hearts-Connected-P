# Builds KHC.exe, the only executable left in the output folder. The room runs
# inside it (it restarts itself with a program name). Other executables that
# earlier Cargo builds left there are removed; Cargo writes them again from its
# cache when asked for them.
param([switch]$Debug, [switch]$Timings)

$ErrorActionPreference = 'Stop'
$env:PATH = (Join-Path $env:USERPROFILE '.cargo/bin') + [IO.Path]::PathSeparator + $env:PATH
$launcherProfile = if ($Debug) { 'dev' } else { 'release' }
Push-Location (Split-Path $PSScriptRoot -Parent)
try {
    $launcherTargets = @('--bin', 'KHC')
    if ($Timings) {
        $launcherTargets += '--timings'
    }
    $buildOutput = & cargo build -p khc @launcherTargets --profile $launcherProfile --locked --message-format=json-render-diagnostics
    if ($LASTEXITCODE -ne 0) { throw 'KHC build failed.' }
    $artifact = $buildOutput | ForEach-Object { $_ | ConvertFrom-Json } |
        Where-Object { $_.reason -eq 'compiler-artifact' -and $_.target.name -eq 'KHC' -and $_.executable } |
        Select-Object -Last 1
    if (-not $artifact) { throw 'Cargo did not report KHC.exe.' }
    $outputDirectory = Split-Path $artifact.executable -Parent
    Get-ChildItem -LiteralPath $outputDirectory -Filter '*.exe' -File |
        Where-Object { $_.Name -ne 'KHC.exe' } |
        ForEach-Object {
            try {
                foreach ($companion in '.pdb', '.d') {
                    $path = Join-Path $outputDirectory ($_.BaseName + $companion)
                    if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Force }
                }
                Remove-Item -LiteralPath $_.FullName -Force
            } catch {
                Write-Warning "Could not remove $($_.TargetObject): $($_.Exception.Message)"
            }
        }
    Write-Output $artifact.executable
} finally {
    Pop-Location
}
