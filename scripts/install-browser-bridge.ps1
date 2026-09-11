param(
    [string]$RuntimeDir = "$env:LOCALAPPDATA\Programs\VibeDictate",
    [string]$ExtensionId = "jjlhamjlfcjmfcjhbokpjbjejendhnoj"
)
$ErrorActionPreference = 'Stop'
$sourceRoot = Split-Path -Parent $PSScriptRoot
$sourceExtension = Join-Path $sourceRoot 'browser-extension'
$runtimeExe = Join-Path $RuntimeDir 'vibe-dictate.exe'
$nativeExe = Join-Path $RuntimeDir 'vibe-dictate-native-host.exe'
$runtimeExtension = Join-Path $RuntimeDir 'browser-extension'
$manifestPath = Join-Path $RuntimeDir 'com.brstk.vibe_dictate.json'
if (!(Test-Path $runtimeExe)) { throw "Runtime executable not found: $runtimeExe" }
if (!(Test-Path $sourceExtension)) { throw "Extension source not found: $sourceExtension" }
New-Item -ItemType Directory -Force -Path $runtimeExtension | Out-Null
Copy-Item (Join-Path $sourceExtension '*') $runtimeExtension -Recurse -Force
Copy-Item $runtimeExe $nativeExe -Force
$manifest = [ordered]@{
    name = 'com.brstk.vibe_dictate'
    description = 'Vibe Dictate browser native messaging bridge'
    path = $nativeExe
    type = 'stdio'
    allowed_origins = @("chrome-extension://$ExtensionId/")
}
$utf8 = New-Object System.Text.UTF8Encoding($false)
[System.IO.File]::WriteAllText($manifestPath, ($manifest | ConvertTo-Json -Depth 4), $utf8)
$chromeKey = 'HKCU:\Software\Google\Chrome\NativeMessagingHosts\com.brstk.vibe_dictate'
$edgeKey = 'HKCU:\Software\Microsoft\Edge\NativeMessagingHosts\com.brstk.vibe_dictate'
foreach ($key in @($chromeKey, $edgeKey)) {
    New-Item -Force -Path $key | Out-Null
    Set-Item -Path $key -Value $manifestPath
}
Write-Output "Native messaging host registered."
Write-Output "Extension ID: $ExtensionId"
Write-Output "Load unpacked folder: $runtimeExtension"
Write-Output "Chrome: chrome://extensions"
Write-Output "Edge: edge://extensions"
