# play.ps1 with the settings measured on an RTX 4070 Super 12 GB (i9-12900K, 32 GB),
# in the prison cell (2026-11-26):
#   VICTORY_CARD_BUFFERS_MIN_MB=1 / _MB=2048  title buffers of 1 MB and more in the card's memory
#                                             (the 12 GB default, 16 MB / 512 MB, leaves them in the
#                                             PC's memory: the card read them over PCIe, ~190 ms a frame)
#   VICTORY_PLACED_ORDER=1                    fewer waits for flights (76 -> 20-44 ms a frame)
#   VICTORY_DYNAMIC_SLOTS=400000              more dynamic view slots under NVIDIA's 1,000,000-view heap:
#                                             no draws dropped for want of them (the flicker when turning)
#   VICTORY_INDIRECT_WIDE=2                   tile dispatches of more than 65,535 groups run whole on NVIDIA
#                                             too (cut, the lowest tiles stayed stale: blocky patches)
#   VICTORY_TEXTURE_DROP_LEVELS=1 / _MIN=512  textures wider than 512 texels a level smaller (a quarter of
#                                             the bytes): the title's textures did not fit in 12 GB (the card
#                                             at 96 %, 49,000 textures never made: holes and black dots);
#                                             now 2,000 wait, the card stays at ~89 % with a stream running
#   VICTORY_TEXTURE_ROOM_PERCENT=90           textures picked by index made up to 90 % of the card's budget
#   (VICTORY_CARD_WINDOWS_MB=1024 left out: the same fps without it, and its ~1 GB went to textures)
#   scripts\play-nvidia.ps1 -Exe E:\xbox\GAME.exe
param(
    [Parameter(Mandatory)][string]$Exe,
    [switch]$Safe,
    [switch]$RayTracing,
    [ValidateSet('diag', 'release')][string]$Build = 'diag',
    [hashtable]$More = @{}
)
$nvidia = @{
    VICTORY_CARD_BUFFERS_MIN_MB = '1'; VICTORY_CARD_BUFFERS_MB = '2048'
    VICTORY_PLACED_ORDER = '1'; VICTORY_DYNAMIC_SLOTS = '400000'
    VICTORY_INDIRECT_WIDE = '2'; VICTORY_TEXTURE_ROOM_PERCENT = '90'
    VICTORY_TEXTURE_DROP_LEVELS = '1'; VICTORY_TEXTURE_DROP_MIN = '512'
}
foreach ($k in $More.Keys) { $nvidia[$k] = $More[$k] }
# Every run writes down the pipelines it draws with, and those of earlier runs are
# made before the title starts (`victory warm --used`: quick once the driver's cache has them),
# so that no pipeline is made in play (on NVIDIA one can take seconds).
$nvidia.VICTORY_PIPELINE_BOOK = '1'
$warm = Join-Path $PSScriptRoot "..\target\$Build\victory.exe"
if (Test-Path $warm) {
    Write-Host 'Making the pipelines of earlier runs (victory warm --used)...'
    Push-Location (Join-Path $PSScriptRoot '..')
    try { & $warm warm --used } finally { Pop-Location }
}
& (Join-Path $PSScriptRoot 'play.ps1') -Exe $Exe -Safe:$Safe -RayTracing:$RayTracing -Build $Build -More $nvidia
