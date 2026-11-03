<#
.SYNOPSIS
Starts one side of a two-PC duel with its network log on, for the KHC-031 checklist.

.DESCRIPTION
Run it on each PC with the same -Run name. The log goes to <Out>\<Run>-<Role>.net.jsonl, one JSON line a second
(rtt_ms, packet_loss, data rates, silent_ms, skipped, snapshots, input_backlog). -Summary reads a log instead and
prints what the checklist asks for.

.EXAMPLE
.\tools\Run-TwoPc.ps1 -Role Host -Run wifi-kh2 -Name PC1 -Room room.txt

.EXAMPLE
.\tools\Run-TwoPc.ps1 -Role Join -Address 192.168.1.20 -Run wifi-kh2 -Name PC2 -Room room.txt

.EXAMPLE
.\tools\Run-TwoPc.ps1 -Summary -Log .local\two-pc\wifi-kh2-Host.net.jsonl
#>
[CmdletBinding(DefaultParameterSetName = 'Play')]
param(
    [Parameter(ParameterSetName = 'Play', Mandatory)][ValidateSet('Host', 'Join')][string]$Role,
    [Parameter(ParameterSetName = 'Play')][string]$Address,
    [Parameter(ParameterSetName = 'Play', Mandatory)][string]$Run,
    [Parameter(ParameterSetName = 'Play', Mandatory)][string]$Name,
    # A text file with one line per room argument: the Steam folder, the manifest and the config, as the launcher
    # passes them (kept out of the repository: they are paths on this PC).
    [Parameter(ParameterSetName = 'Play', Mandatory)][string]$Room,
    [Parameter(ParameterSetName = 'Play')][ValidateSet('kh2', 'kh3', 'each')][string]$Ruleset = 'kh2',
    [Parameter(ParameterSetName = 'Play')][string]$Out = '.local\two-pc',
    [Parameter(ParameterSetName = 'Play')][string]$Exe = 'target\release\KHC.exe',
    [Parameter(ParameterSetName = 'Summary', Mandatory)][switch]$Summary,
    [Parameter(ParameterSetName = 'Summary', Mandatory)][string]$Log
)

$ErrorActionPreference = 'Stop'

if ($Summary) {
    $lines = Get-Content -LiteralPath $Log | ForEach-Object { $_ | ConvertFrom-Json }
    if (-not $lines) { throw "$Log has no lines" }
    $rtt = @($lines | ForEach-Object { [double]$_.rtt_ms } | Sort-Object)
    $at = { param($p) $rtt[[Math]::Min($rtt.Count - 1, [int][Math]::Floor($rtt.Count * $p))] }
    $skipped = ($lines | Measure-Object -Property skipped -Maximum).Maximum
    [pscustomobject]@{
        Seconds     = $lines.Count
        MedianRttMs = [Math]::Round((& $at 0.5), 1)
        P99RttMs    = [Math]::Round((& $at 0.99), 1)
        MaxLoss     = ($lines | Measure-Object -Property packet_loss -Maximum).Maximum
        Skipped     = $skipped
    } | Format-List
    return
}

if ($Role -eq 'Join' -and -not $Address) { throw 'A joiner needs -Address, the host PC''s IP address' }
if (-not (Test-Path -LiteralPath $Exe)) { throw "$Exe is not built: run build.bat first" }
$roomArgs = @(Get-Content -LiteralPath $Room | Where-Object { $_.Trim() })
if ($roomArgs.Count -ne 3) { throw "$Room needs three lines: Steam folder, manifest, config" }
New-Item -ItemType Directory -Force $Out | Out-Null
$logFile = Join-Path $Out "$Run-$Role.net.jsonl"
$arguments = @('room') + $roomArgs + @('--net-log', $logFile, '--net-overlay', '--name', $Name)
if ($Role -eq 'Host') {
    $arguments += @('--host', '--duel-mode', 'best3', '--duel-ruleset', $Ruleset)
} else {
    $arguments += @('--join', $Address)
}
$quoted = $arguments | ForEach-Object { if ($_ -match '\s') { '"' + $_ + '"' } else { $_ } }
Write-Host "Network log: $logFile"
$process = Start-Process -FilePath $Exe -ArgumentList $quoted -PassThru
$process.WaitForExit()
Write-Host "Exit code $($process.ExitCode). Summary:"
& $PSCommandPath -Summary -Log $logFile
