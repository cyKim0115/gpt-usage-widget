# Launches the standalone widget for non-developers.
# Rebuilds release when source is newer than the release exe.
# Use -ForceRebuild to always rebuild before install (agent / post-task refresh).

param(
  [switch]$ForceRebuild
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
if (-not (Test-Path (Join-Path $Root "package.json"))) {
  $Root = $PSScriptRoot
}

$InstallDir = Join-Path $env:LOCALAPPDATA "GPTUsageWidget"
$InstallExe = Join-Path $InstallDir "gpt-usage-widget.exe"
$ReleaseExe = Join-Path $Root "src-tauri\target\release\gpt-usage-widget.exe"
$ProcessName = "gpt-usage-widget"

function Show-Error([string]$Message) {
  Add-Type -AssemblyName PresentationFramework | Out-Null
  [System.Windows.MessageBox]::Show($Message, "GPT Usage Widget", "OK", "Error") | Out-Null
}

function Ensure-VcEnv {
  $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
  $vsPath = $null
  if (Test-Path $vswhere) {
    $vsPath = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2>$null
  }
  if (-not $vsPath) {
    $fallback = "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools"
    if (Test-Path $fallback) { $vsPath = $fallback }
  }
  if (-not $vsPath) { return $null }
  $bat = Join-Path $vsPath "VC\Auxiliary\Build\vcvars64.bat"
  if (-not (Test-Path $bat)) { return $null }
  return $bat
}

function Get-SourceStamp {
  $paths = @(
    (Join-Path $Root "package.json"),
    (Join-Path $Root "index.html"),
    (Join-Path $Root "settings.html"),
    (Join-Path $Root "package-lock.json"),
    (Join-Path $Root "tsconfig.json"),
    (Join-Path $Root "vite.config.ts"),
    (Join-Path $Root "src-tauri\Cargo.toml"),
    (Join-Path $Root "src-tauri\Cargo.lock"),
    (Join-Path $Root "src-tauri\build.rs"),
    (Join-Path $Root "src-tauri\tauri.conf.json")
  )
  $latest = [datetime]::MinValue
  foreach ($p in $paths) {
    if (Test-Path $p) {
      $t = (Get-Item $p).LastWriteTimeUtc
      if ($t -gt $latest) { $latest = $t }
    }
  }
  foreach ($dir in @("src", "src-tauri\src", "src-tauri\capabilities", "src-tauri\icons")) {
    $full = Join-Path $Root $dir
    if (-not (Test-Path $full)) { continue }
    Get-ChildItem $full -Recurse -File -ErrorAction SilentlyContinue | ForEach-Object {
      if ($_.LastWriteTimeUtc -gt $latest) { $latest = $_.LastWriteTimeUtc }
    }
  }
  return $latest
}

function Test-NeedsRebuild {
  if ($ForceRebuild) { return $true }
  if (-not (Test-Path $ReleaseExe)) { return $true }
  $builtAt = (Get-Item $ReleaseExe).LastWriteTimeUtc
  $sourceAt = Get-SourceStamp
  return $sourceAt -gt $builtAt
}

function Get-ExeHash([string]$Path) {
  # 기본 Windows PowerShell에서 Get-FileHash 모듈이 없는 환경도 지원합니다.
  $hasher = [System.Security.Cryptography.SHA256]::Create()
  $stream = [System.IO.File]::OpenRead($Path)
  try {
    return [Convert]::ToBase64String($hasher.ComputeHash($stream))
  } finally {
    $stream.Dispose()
    $hasher.Dispose()
  }
}

function Build-Release {
  $vcvars = Ensure-VcEnv
  if (-not $vcvars) {
    Show-Error "Release build needs Visual Studio C++ Build Tools.`n`nRun once in a dev shell:`n  npm run build:app`n`nThen double-click start.bat again."
    exit 1
  }

  Write-Host "Building release (first time may take a few minutes)..."
  if (-not (Test-Path (Join-Path $Root "node_modules"))) {
    Push-Location $Root
    npm install
    Pop-Location
    if ($LASTEXITCODE -ne 0) {
      Show-Error "npm install failed."
      exit 1
    }
  }
  $before = if (Test-Path $ReleaseExe) { (Get-Item $ReleaseExe).LastWriteTimeUtc } else { [datetime]::MinValue }

  $cmd = "`"$vcvars`" && cd /d `"$Root`" && npm run build:app"
  cmd /c $cmd

  # 존재 검사만으로는 부족합니다. 빌드가 실패해도 이전 exe가 남아 있어
  # 그대로 통과하고, 낡은 실행 파일이 새것처럼 설치됩니다.
  $after = if (Test-Path $ReleaseExe) { (Get-Item $ReleaseExe).LastWriteTimeUtc } else { [datetime]::MinValue }
  if ($LASTEXITCODE -ne 0 -or $after -le $before) {
    Show-Error "Build failed. Check Node.js / Rust / VS Build Tools."
    exit 1
  }
}

function Stop-RunningWidget {
  Get-Process -Name $ProcessName -ErrorAction SilentlyContinue | ForEach-Object {
    Write-Host "Stopping running widget (PID $($_.Id))..."
    Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue
    Start-Sleep -Milliseconds 400
  }
}

if (Test-NeedsRebuild) {
  # 빌드보다 먼저 끕니다. 실행 중이면 Windows가 exe에 쓰기 잠금을 걸어
  # cargo가 링크 단계에서 os error 5로 실패합니다.
  Stop-RunningWidget
  Build-Release
}

if (-not (Test-Path $ReleaseExe)) {
  Show-Error "Release exe not found. Run npm run build:app first."
  exit 1
}

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
$InstallNeeded = -not (Test-Path -LiteralPath $InstallExe)
if (-not $InstallNeeded) {
  $InstallNeeded = (Get-ExeHash $ReleaseExe) -ne (Get-ExeHash $InstallExe)
}
if ($InstallNeeded) {
  Stop-RunningWidget
  # Windows caches icons by path — replace via temp name so Explorer picks up the new PE icon.
  $tmp = Join-Path $InstallDir "gpt-usage-widget.new.exe"
  Copy-Item -LiteralPath $ReleaseExe -Destination $tmp -Force
  if (Test-Path -LiteralPath $InstallExe) { Remove-Item -LiteralPath $InstallExe -Force }
  Rename-Item -LiteralPath $tmp -NewName (Split-Path $InstallExe -Leaf) -Force
  Write-Host "Installed: $InstallExe"
  Remove-Item -LiteralPath (Join-Path $InstallDir "icon.ico") -Force -ErrorAction SilentlyContinue
}

try {
  $desktop = [Environment]::GetFolderPath("Desktop")
  $lnkPath = Join-Path $desktop "GPT Usage Widget.lnk"
  $w = New-Object -ComObject WScript.Shell
  $lnk = $w.CreateShortcut($lnkPath)
  $lnk.TargetPath = $InstallExe
  $lnk.WorkingDirectory = $InstallDir
  $lnk.Description = "GPT Usage floating widget"
  $lnk.IconLocation = "$InstallExe,0"
  $lnk.Save()
} catch {
  # ignore shortcut failures
}

# 사용자가 조작하는 위젯 창이므로 표시 상태로 실행합니다 (콘솔 없는 릴리스 exe).
Start-Process -FilePath $InstallExe -WindowStyle Normal
exit 0
