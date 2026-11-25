# Builds Launcher.exe with the C# compiler that ships with Windows
# (.NET Framework 4). Output: <repo>\dist\Launcher.exe, with a
# credits.txt template next to it the first time.
$ErrorActionPreference = 'Stop'
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$repo = Split-Path -Parent $here
$dist = Join-Path $repo 'dist'
New-Item -ItemType Directory -Force $dist | Out-Null
$csc = Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
& $csc /nologo /target:winexe /optimize+ /codepage:65001 `
    "/out:$(Join-Path $dist 'Launcher.exe')" `
    /reference:System.Windows.Forms.dll /reference:System.Drawing.dll `
    (Join-Path $here 'Launcher.cs')
if ($LASTEXITCODE -ne 0) { throw "csc failed ($LASTEXITCODE)" }
$credits = Join-Path $dist 'credits.txt'
if (-not (Test-Path $credits)) {
    Copy-Item (Join-Path $here 'credits.txt') $credits
}
Write-Output "built $(Join-Path $dist 'Launcher.exe')"
