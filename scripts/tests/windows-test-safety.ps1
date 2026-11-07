# The safety of one browser test run, shared by every runner that starts a browser.
#
#   . scripts/tests/windows-test-safety.ps1
#   $safety = Start-RunSafety -OutDir $OutDir -Name features [-Alone]
#   Assert-RunSafe $safety      # before each step; exits when the guard stopped the run
#   Stop-RunSafety $safety      # at the end
#
# Start-RunSafety does four things before any browser starts:
#
# 1. Sets RIVET_KEEP_OFFSCREEN for the run and everything it starts. Under it the Rivet runtime keeps
#    every top-level window of the browser off every monitor and never activates one, the setup
#    window shows off-screen without activation, and nothing brings a window forward.
# 2. Sets RIVET_DATA_ROOT to the run's own folder, so the sessions, spaces and tab lists the run
#    makes never land in the person's own Rivet folder. The applied signed build table is copied in,
#    so the runtime sees the same rows an installed build would.
# 3. Registers the run below .scratch\runs. A run that must be alone, such as one with the
#    open-link rows, is refused while another run is active, and no run starts beside it.
# 4. Starts windows-visible-guard.ps1 on the run's folder. The guard stops this run, and only this
#    run, when a test window shows on a monitor, takes the foreground or holds the mouse.

$script:SafetyRepo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path

function Get-RunRegistry { Join-Path $script:SafetyRepo '.scratch\runs' }

# The runs registered and still running, each with its process id, start time, folder and whether
# it must be alone. A record whose process is gone, or whose id another process took, is removed.
function Get-ActiveRuns {
  $dir = Get-RunRegistry
  if (-not (Test-Path $dir)) { return @() }
  $live = @()
  foreach ($file in Get-ChildItem $dir -Filter '*.json' -ErrorAction SilentlyContinue) {
    $record = $null
    try { $record = Get-Content -Raw $file.FullName | ConvertFrom-Json } catch { }
    $proc = if ($record) { Get-Process -Id ([int]$record.pid) -ErrorAction SilentlyContinue } else { $null }
    if ($proc -and $proc.StartTime.ToUniversalTime().Ticks -eq [long]$record.start) { $live += $record }
    else { Remove-Item $file.FullName -ErrorAction SilentlyContinue }
  }
  $live
}

# Whether a run that `wantsAlone` may start beside the `active` runs, and why not.
function Test-RunMayStart([bool]$WantsAlone, $Active) {
  $others = @($Active | Where-Object { $_ })
  $alone = @($others | Where-Object { $_.alone })
  if ($alone.Count) { return @($false, "the run in $($alone[0].out) (pid $($alone[0].pid)) must run alone") }
  if ($WantsAlone -and $others.Count) { return @($false, "$($others.Count) other run(s) are active, the first in $($others[0].out) (pid $($others[0].pid))") }
  @($true, '')
}

function Start-RunSafety {
  param(
    [Parameter(Mandatory = $true)][string]$OutDir,
    [Parameter(Mandatory = $true)][string]$Name,
    # The run holds rows that must not run beside another run.
    [switch]$Alone,
    # When the run cannot be alone, it starts without those rows instead of not at all.
    [switch]$DropAloneRows
  )
  $env:RIVET_KEEP_OFFSCREEN = '1'
  $dataRoot = Join-Path $OutDir 'rivet-data'
  New-Item -ItemType Directory -Force $dataRoot | Out-Null
  $env:RIVET_DATA_ROOT = $dataRoot
  # The signed build table is Rivet's own cache, not browser data. An installed build would read it.
  if ($env:LOCALAPPDATA) {
    $table = Join-Path $env:LOCALAPPDATA 'Rivet\cache\builds\table.bin'
    $copy = Join-Path $dataRoot 'Rivet\cache\builds'
    if (Test-Path $table) {
      New-Item -ItemType Directory -Force $copy | Out-Null
      Copy-Item $table (Join-Path $copy 'table.bin') -Force
    }
  }

  # The registry is read and written under one lock, so two runs starting together see each other.
  $lock = New-Object System.Threading.Mutex($false, 'Local\RivetTestRunRegistry')
  [void]$lock.WaitOne()
  $alone = [bool]$Alone
  $note = ''
  try {
    $ok, $why = Test-RunMayStart $alone (Get-ActiveRuns)
    if (-not $ok -and $alone -and $DropAloneRows) {
      $ok2, $why2 = Test-RunMayStart $false (Get-ActiveRuns)
      if ($ok2) { $note = $why; $alone = $false; $ok = $true }
      else { $why = $why2 }
    }
    if (-not $ok) {
      Write-Host "refusing to start the $Name run: $why"
      exit 2
    }
    $dir = Get-RunRegistry
    New-Item -ItemType Directory -Force $dir | Out-Null
    $me = Get-Process -Id $PID
    $record = Join-Path $dir "$PID.json"
    @{ pid = $PID; start = $me.StartTime.ToUniversalTime().Ticks; out = $OutDir; name = $Name; alone = $alone } |
      ConvertTo-Json -Compress | Set-Content -Path $record -Encoding utf8
  } finally { $lock.ReleaseMutex(); $lock.Dispose() }

  $log = Join-Path $OutDir 'guard.log'
  $fail = Join-Path $OutDir 'guard-failed.txt'
  Remove-Item $log, $fail -ErrorAction SilentlyContinue
  $guardArgs = @('-NoProfile', '-File', (Join-Path $PSScriptRoot 'windows-visible-guard.ps1'), '-Mark', $OutDir, '-Log', $log, '-FailFile', $fail, '-RunnerPid', $PID)
  $guard = Start-Process -FilePath (Get-Process -Id $PID).Path -ArgumentList $guardArgs -WindowStyle Hidden -PassThru
  # No browser starts before the guard watches.
  foreach ($i in 1..100) { if (Test-Path $log) { break }; Start-Sleep -Milliseconds 100 }
  if (-not (Test-Path $log)) {
    Stop-Process -Id $guard.Id -Force -ErrorAction SilentlyContinue
    Remove-Item $record -ErrorAction SilentlyContinue
    Write-Host "refusing to start the $Name run: the visible guard did not start"
    exit 2
  }
  [pscustomobject]@{ Guard = $guard; Log = $log; FailFile = $fail; Record = $record; DataRoot = $dataRoot; Alone = $alone; AloneRefused = $note }
}

# Stops the run when the guard found a breach or is no longer watching. The guard has already
# stopped the run's test processes; this ends the runner before it starts another browser.
function Assert-RunSafe($Safety) {
  if (-not $Safety) { return }
  $why = $null
  if (Test-Path $Safety.FailFile) { $why = Get-Content -Raw $Safety.FailFile }
  elseif ($Safety.Guard.HasExited) { $why = "the visible guard exited with $($Safety.Guard.ExitCode)" }
  if ($why) {
    Write-Host "run stopped by the visible guard: $why"
    Remove-Item $Safety.Record -ErrorAction SilentlyContinue
    exit 3
  }
}

function Stop-RunSafety($Safety) {
  if (-not $Safety) { return }
  Assert-RunSafe $Safety
  Stop-Process -Id $Safety.Guard.Id -Force -ErrorAction SilentlyContinue
  Remove-Item $Safety.Record -ErrorAction SilentlyContinue
}
