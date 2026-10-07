@echo off
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul
set PATH=%USERPROFILE%\.cargo\bin;%PATH%
cd /d "%~dp0src-tauri"
cargo test -- --nocapture
