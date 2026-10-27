$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot -Parent
Set-Location -LiteralPath $projectRoot
$mobileRoot = Join-Path $projectRoot 'apps\mobile'
$generatedResources = Join-Path $mobileRoot 'src-tauri\gen\android\app\src\main\res'
if (-not (Test-Path -LiteralPath $generatedResources)) {
    throw 'Сначала подготовьте Android-проект: powershell -File scripts/android.ps1 -Mode Init'
}

Copy-Item -LiteralPath (Join-Path $projectRoot 'apps\frontend\public\design\synapse-logo.svg') -Destination (Join-Path $mobileRoot 'icon.svg')
# Поля 22/108 сохраняют весь знак внутри круглой и квадратной масок Android.
[xml]$logo = Get-Content -LiteralPath (Join-Path $mobileRoot 'icon.svg') -Raw -Encoding UTF8
$logo.DocumentElement.SetAttribute('x', '22')
$logo.DocumentElement.SetAttribute('y', '22')
$logo.DocumentElement.SetAttribute('width', '64')
$logo.DocumentElement.SetAttribute('height', '64')
$canvas = '<svg xmlns="http://www.w3.org/2000/svg" width="108" height="108" viewBox="0 0 108 108">'
$monochrome = $canvas + $logo.DocumentElement.OuterXml + '</svg>'
[IO.File]::WriteAllText((Join-Path $mobileRoot 'icon-monochrome.svg'), $monochrome, [Text.UTF8Encoding]::new($false))
# Белый рисунок внутри знака сохраняется, а наружная подложка совпадает с цветом логотипа.
$foreground = $canvas + '<circle cx="54.9" cy="54.3" r="27" fill="#ffffff" />' + $logo.DocumentElement.OuterXml + '</svg>'
[IO.File]::WriteAllText((Join-Path $mobileRoot 'icon-foreground.svg'), $foreground, [Text.UTF8Encoding]::new($false))
& npm.cmd run mobile -- icon icon.json --output src-tauri/icons
if ($LASTEXITCODE -ne 0) { throw 'Не удалось создать иконки СИНАПС.' }

# Tauri записывает Android-иконки непосредственно в gen. Сохраняем их вне gen,
# откуда build.rs восстанавливает ресурсы при последующих сборках и android init.
$savedResources = Join-Path $mobileRoot 'src-tauri\icons\android'
New-Item -ItemType Directory -Path $savedResources -Force | Out-Null
Get-ChildItem -LiteralPath $generatedResources -Directory |
    Where-Object { $_.Name -like 'mipmap-*' } |
    ForEach-Object { Copy-Item -LiteralPath $_.FullName -Destination $savedResources -Recurse -Force }
$savedValues = Join-Path $savedResources 'values'
New-Item -ItemType Directory -Path $savedValues -Force | Out-Null
Copy-Item -LiteralPath (Join-Path $generatedResources 'values\ic_launcher_background.xml') -Destination $savedValues -Force
Write-Output 'Иконки СИНАПС созданы и сохранены для следующих сборок.'
