@echo off
setlocal
cd /d "%~dp0"
set "ELEVAGE_DATA=%LOCALAPPDATA%\EO-Suivi-Elevage\data"
if not exist "%ELEVAGE_DATA%" mkdir "%ELEVAGE_DATA%"
start "EO-Suivi Rust" /min EO-Suivi-Rust.exe
timeout /t 2 /nobreak >nul
start "" http://127.0.0.1:8080

