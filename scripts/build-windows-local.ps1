param(
    [ValidateSet('check','test','release')]
    [string]$Action = 'release'
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$sysroot = Join-Path $env:LOCALAPPDATA 'cargo-xwin\windows-msvc-sysroot\windows-msvc-sysroot'
$cargo = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
$llvm = 'C:\Program Files\LLVM\bin'
if (!(Test-Path $cargo)) { throw 'Rust/Cargo is not installed for this user.' }
if (!(Test-Path (Join-Path $llvm 'clang-cl.exe'))) { throw 'LLVM clang-cl is not installed.' }
if (!(Test-Path (Join-Path $sysroot 'include'))) { throw 'Windows MSVC sysroot is missing.' }
$target = 'x86_64-pc-windows-msvc'
$env:PATH = "$llvm;$env:USERPROFILE\.cargo\bin;$env:PATH"
$env:LIB = Join-Path $sysroot "lib\x86_64-unknown-windows-msvc"
$env:INCLUDE = Join-Path $sysroot 'include'
$env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER = Join-Path $llvm 'lld-link.exe'
$env:CC_x86_64_pc_windows_msvc = Join-Path $llvm 'clang-cl.exe'
$env:CXX_x86_64_pc_windows_msvc = Join-Path $llvm 'clang-cl.exe'
$env:CFLAGS_x86_64_pc_windows_msvc = "/I$sysroot\include"
$env:CXXFLAGS_x86_64_pc_windows_msvc = "/I$sysroot\include /I$sysroot\include\c++\msstl"
Set-Location $repo
switch ($Action) {
    'check'   { & $cargo check --target $target }
    'test'    { & $cargo test --target $target }
    'release' { & $cargo build --release --target $target }
}
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
if ($Action -eq 'release') {
    $exe = Join-Path $repo 'target\x86_64-pc-windows-msvc\release\vibe-dictate.exe'
    Get-Item $exe | Select-Object FullName, Length, LastWriteTime
    Get-FileHash $exe -Algorithm SHA256 | Select-Object Hash
}
