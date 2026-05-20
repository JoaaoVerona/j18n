$ErrorActionPreference = 'Stop'
# Invoke-WebRequest renders a progress bar that is extremely slow when stdout is
# redirected (i.e. run non-interactively) on Windows PowerShell 5.1 — suppress
# it so the download doesn't appear to hang.
$ProgressPreference = 'SilentlyContinue'

$repo = 'Skiley/j18n'
$installDir = if ($env:J18N_INSTALL_DIR) { $env:J18N_INSTALL_DIR } else { "$env:LOCALAPPDATA\j18n\bin" }
# Version precedence: positional arg, then $env:J18N_VERSION (the `iwr ... | iex`
# invocation form can't pass positional args), then latest.
$version = if ($args[0]) { $args[0] } elseif ($env:J18N_VERSION) { $env:J18N_VERSION } else { 'latest' }

$arch = switch ($env:PROCESSOR_ARCHITECTURE) {
  'AMD64' { 'x86_64' }
  'ARM64' { 'aarch64' }
  default { throw "j18n: unsupported architecture: $env:PROCESSOR_ARCHITECTURE" }
}

$target = "$arch-pc-windows-msvc"
$archive = "j18n-cli-$target.zip"

$url = if ($version -eq 'latest') {
  "https://github.com/$repo/releases/latest/download/$archive"
} else {
  "https://github.com/$repo/releases/download/$version/$archive"
}

$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ([guid]::NewGuid())
New-Item -ItemType Directory -Path $tmp -Force | Out-Null

try {
  Write-Host "Downloading $archive..."
  Invoke-WebRequest -Uri $url -OutFile (Join-Path $tmp $archive) -UseBasicParsing
  Expand-Archive -Path (Join-Path $tmp $archive) -DestinationPath $tmp

  New-Item -ItemType Directory -Path $installDir -Force | Out-Null
  $dest = Join-Path $installDir 'j18n.exe'

  # Windows won't let you overwrite a running .exe, but it WILL let you rename
  # one. Move any existing binary aside first so an in-place update (which runs
  # this script while j18n.exe may be executing) works. Delete any stale .old
  # from a previous update first (it's a dead file by now and safe to remove).
  if (Test-Path $dest) {
    $old = "$dest.old"
    Remove-Item -Path $old -Force -ErrorAction SilentlyContinue
    Rename-Item -Path $dest -NewName 'j18n.exe.old' -ErrorAction SilentlyContinue
  }
  Move-Item -Path (Join-Path $tmp "j18n-cli-$target\j18n.exe") -Destination $dest -Force

  # Try to drop the .old now. Succeeds on a manual upgrade (the old binary
  # isn't running), so that path leaves no litter.
  $old = "$dest.old"
  if (Test-Path $old) { Remove-Item -Path $old -Force -ErrorAction SilentlyContinue }

  Write-Host "Installed j18n.exe to $dest"

  $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
  if (-not ($userPath -split ';' -contains $installDir)) {
    [Environment]::SetEnvironmentVariable('Path', "$userPath;$installDir", 'User')
    Write-Host ""
    Write-Host "Added $installDir to your PATH (open a new shell to use)."
  }
} finally {
  Remove-Item -Path $tmp -Recurse -Force -ErrorAction SilentlyContinue
}
