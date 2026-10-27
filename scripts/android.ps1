param(
    [ValidateSet('Run', 'Build', 'Init')]
    [string]$Mode = 'Run',
    [ValidateSet('x86_64', 'aarch64', 'armv7', 'i686')]
    [string]$Architecture = 'x86_64',
    [string]$ApiUrl = 'http://10.0.2.2:3000',
    [string]$DeviceId = ''
)

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot -Parent
Set-Location -LiteralPath $projectRoot

# Настройки действуют только в процессе сборки и не меняют глобальное окружение Windows.
if (-not $env:ANDROID_HOME) { $env:ANDROID_HOME = Join-Path $env:LOCALAPPDATA 'Android\Sdk' }
if (-not $env:JAVA_HOME) { $env:JAVA_HOME = 'C:\Program Files\Android\Android Studio\jbr' }
if (-not (Test-Path -LiteralPath (Join-Path $env:JAVA_HOME 'bin\java.exe'))) { throw 'Не найден JDK. Настройте JAVA_HOME.' }
$adbPath = Join-Path $env:ANDROID_HOME 'platform-tools\adb.exe'
if (-not (Test-Path -LiteralPath $adbPath)) { throw 'Не найдены Android SDK Platform-Tools. Проверьте ANDROID_HOME.' }
if (-not $env:NDK_HOME) {
    $ndkRoot = Join-Path $env:ANDROID_HOME 'ndk'
    $ndk = Get-ChildItem -LiteralPath $ndkRoot -Directory -ErrorAction SilentlyContinue |
        Where-Object { Test-Path -LiteralPath (Join-Path $_.FullName 'source.properties') } |
        Sort-Object { [version]$_.Name } -Descending | Select-Object -First 1
    if ($ndk) { $env:NDK_HOME = $ndk.FullName }
}
if (-not $env:NDK_HOME -or -not (Test-Path -LiteralPath (Join-Path $env:NDK_HOME 'source.properties'))) {
    throw 'Не найден Android NDK. Установите NDK (Side by side) в SDK Manager.'
}
$env:Path = "$(Join-Path $env:JAVA_HOME 'bin');$(Join-Path $env:ANDROID_HOME 'platform-tools');$env:Path"
$env:SYNAPSE_API_URL = $ApiUrl

# Линкер NDK в Windows некорректно обрабатывает кириллицу в путях объектных файлов.
# Отдельный ASCII-кеш оставляет исходный репозиторий на месте и подходит дочерним вызовам Cargo.
if ($projectRoot -match '[^\x00-\x7F]') {
    $env:CARGO_TARGET_DIR = Join-Path $env:LOCALAPPDATA 'Synapse\build\android-target'
    if ($env:CARGO_TARGET_DIR -match '[^\x00-\x7F]') { throw 'Для Android-сборки требуется ASCII-путь CARGO_TARGET_DIR.' }
    New-Item -ItemType Directory -Path $env:CARGO_TARGET_DIR -Force | Out-Null
}

$androidProject = Join-Path $projectRoot 'apps\mobile\src-tauri\gen\android'
if (-not (Test-Path -LiteralPath (Join-Path $androidProject 'gradlew.bat'))) {
    & npm.cmd run mobile -- android init --ci --skip-targets-install
    if ($LASTEXITCODE -ne 0) { throw 'Не удалось инициализировать Android-проект.' }
}

# Gradle запрещает не-ASCII пути по умолчанию; разрешаем существующий путь этого репозитория.
$gradleProperties = Join-Path $androidProject 'gradle.properties'
if ($projectRoot -match '[^\x00-\x7F]') {
    $properties = Get-Content -LiteralPath $gradleProperties -Raw
    if ($properties -notmatch '(?m)^android\.overridePathCheck=true\s*$') {
        Add-Content -LiteralPath $gradleProperties -Value "`nandroid.overridePathCheck=true" -Encoding UTF8
    }
}
if ($Mode -eq 'Init') { Write-Output 'Android-проект подготовлен.'; exit 0 }

$rustTarget = switch ($Architecture) {
    'x86_64' { 'x86_64-linux-android' }
    'aarch64' { 'aarch64-linux-android' }
    'armv7' { 'armv7-linux-androideabi' }
    'i686' { 'i686-linux-android' }
}
& rustup target add $rustTarget
if ($LASTEXITCODE -ne 0) { throw 'Не удалось подготовить Android-цель Rust.' }
Write-Output "Сборка для $Architecture. Адрес API внутри приложения: $ApiUrl"
& npm.cmd run mobile -- android build --debug --apk --target $Architecture
if ($LASTEXITCODE -ne 0) { throw 'Сборка APK завершилась ошибкой.' }

$apk = Get-ChildItem -LiteralPath (Join-Path $androidProject 'app\build\outputs\apk') -Filter '*debug.apk' -Recurse |
    Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1
if (-not $apk) { throw 'Сборка завершилась без найденного debug APK.' }
$outputDirectory = Join-Path $projectRoot 'output\android'
New-Item -ItemType Directory -Path $outputDirectory -Force | Out-Null
$outputApk = Join-Path $outputDirectory "synapse-$Architecture-debug.apk"
Copy-Item -LiteralPath $apk.FullName -Destination $outputApk -Force
Write-Output "APK: $outputApk"
if ($Mode -eq 'Build') { exit 0 }

# Устанавливаем только на явно выбранное либо единственное подключённое устройство.
if (-not $DeviceId) {
    $deviceLines = & $adbPath devices
    $devices = @($deviceLines | Where-Object { $_ -match '^\S+\s+device\s*$' } | ForEach-Object { ($_ -split '\s+')[0] })
    if ($devices.Count -ne 1) { throw 'Ожидается одно подключённое устройство. Запустите эмулятор или укажите -DeviceId.' }
    $DeviceId = $devices[0]
}
& $adbPath -s $DeviceId install -r $outputApk
if ($LASTEXITCODE -ne 0) { throw 'Не удалось установить APK на устройство.' }
& $adbPath -s $DeviceId shell am start -n 'com.synapse.compass/.MainActivity'
if ($LASTEXITCODE -ne 0) { throw 'Не удалось запустить приложение.' }
Write-Output 'Приложение запущено. API должен оставаться доступен по указанному адресу.'
