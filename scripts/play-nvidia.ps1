# play.ps1 with the settings for an RTX 4070 Super 12 GB (i9-12900K, 32 GB), measured on the
# 7900XTX build in the prison cell and carried over here (2026-11-26):
#   VICTORY_CARD_BUFFERS_MIN_MB=1 / _MB=2048  title buffers of 1 MB and more in the card's memory: on a
#                                             12 GB card the default is 512 MB of them (14 copies in the
#                                             run to the boat) and the rest is read over PCIe (card ~140 ms
#                                             a frame); on the 7900XTX build this gave 4.4 -> 7-9 fps.
#                                             Copies are only made while the card is under 80 % of its budget.
#   VICTORY_PLACED_ORDER=1                    the per-frame wait for "a fill of upload memory" inside shader
#                                             writes (the title's constant ring, ~100 ms a frame in the run
#                                             to the boat) cuts only the submission behind the copy
#   VICTORY_INDIRECT_WIDE=2                   tile dispatches of more than 65,535 groups run whole on NVIDIA
#                                             too (cut, the lowest tiles stay stale: blocky patches)
#   VICTORY_TEXTURE_DROP_LEVELS=1 / _MIN=2048 only textures wider than 2048 texels (4K) a level smaller
#                                             (a quarter of their bytes, 4K -> 2K): little to see, and video
#                                             memory back. _MIN=1024 or 512 frees more at a visible cost; 0 off.
#   (VICTORY_POOL_FULLEST, on by default: texture blocks are filled fullest first so emptied ones go.)
#   scripts\play-nvidia.ps1 -Exe E:\xbox\<executable>
param(
    [Parameter(Mandatory)][string]$Exe,
    [switch]$Safe,
    [switch]$RayTracing,
    [ValidateSet('diag', 'release')][string]$Build = 'diag',
    [hashtable]$More = @{}
)
$nvidia = @{
    # 1024, not 2048: at 2048 the card went 1.9 GB over its budget (pink foliage in town).
    VICTORY_CARD_BUFFERS_MIN_MB = '1'; VICTORY_CARD_BUFFERS_MB = '1024'
    VICTORY_PLACED_ORDER = '1'; VICTORY_INDIRECT_WIDE = '2'
    VICTORY_TEXTURE_DROP_LEVELS = '1'; VICTORY_TEXTURE_DROP_MIN = '2048'
    VICTORY_LEVELS_ON_DEMAND = '1'
    # Boat scene, 2026-11-26 night: 6 fps -> 10.6 with texture blocks of 16 MB, -> 12.6 with targets
    # kept on the card first when it is over its budget (residency priorities).
    # (VICTORY_POOL_COMPACT=1, emptying mostly-free blocks, is not on: it freed little in run b6.)
    VICTORY_POOL_BLOCK_MB = '16'; VICTORY_RESIDENCY_PRIORITY = '1'
}
foreach ($k in $More.Keys) { $nvidia[$k] = $More[$k] }
# Every run writes down the pipelines it draws with, and those of earlier runs are
# made before the title starts (`victory warm --used`), so that none is made in play.
$nvidia.VICTORY_PIPELINE_BOOK = '1'
$warm = Join-Path $PSScriptRoot "..\target\$Build\victory.exe"
if (Test-Path $warm) {
    Write-Host 'Making the pipelines of earlier runs (victory warm --used)...'
    Push-Location (Join-Path $PSScriptRoot '..')
    try { & $warm warm --used } finally { Pop-Location }
}
& (Join-Path $PSScriptRoot 'play.ps1') -Exe $Exe -Safe:$Safe -RayTracing:$RayTracing -Build $Build -More $nvidia
