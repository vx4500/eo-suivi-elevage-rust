@echo off
chcp 65001 >nul
setlocal
cd /d "%~dp0"

where cargo >nul 2>&1
if errorlevel 1 (
  echo Rust n'est pas installe.
  echo Installez-le depuis https://rustup.rs puis relancez ce fichier.
  pause
  exit /b 1
)

echo Compilation EO-Suivi Rust en mode release...
cargo build --release
if errorlevel 1 (
  echo ECHEC de la compilation.
  pause
  exit /b 1
)

if not exist dist-rust mkdir dist-rust
if not exist dist-rust\static mkdir dist-rust\static
copy /y target\release\eo-suivi-elevage.exe dist-rust\EO-Suivi-Rust.exe >nul
copy /y static\style.css dist-rust\static\style.css >nul
copy /y Lancer-EO-Suivi-Rust.bat dist-rust\Lancer-EO-Suivi-Rust.bat >nul
copy /y README.md dist-rust\README.md >nul

echo.
echo Termine : dist-rust\EO-Suivi-Rust.exe
pause
