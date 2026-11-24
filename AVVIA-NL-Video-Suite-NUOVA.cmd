@echo off
rem Avvia la versione NUOVA di NL Video Suite (filmcraft-nl), non la suite classica.
rem Cerca la build piu recente in distribuzione\NLVideoSuite-*-windows-x64 e la avvia.
setlocal enabledelayedexpansion
set "ROOT=%~dp0"
set "APP="
for /f "delims=" %%D in ('dir /b /ad /o-n "%ROOT%distribuzione\NLVideoSuite-*-windows-x64" 2^>nul') do (
    if not defined APP set "APP=%ROOT%distribuzione\%%D"
)
if not defined APP (
    echo.
    echo Nessuna build trovata in "%ROOT%distribuzione".
    echo Crea il portabile con:  packaging\nl\portable.ps1 -Version 1.1.0
    echo oppure usa INSTALLA-NL-Video-Suite-NUOVA.cmd dopo aver creato il bundle.
    echo.
    pause
    exit /b 1
)
echo Avvio: %APP%
start "NL Video Suite" "%APP%\NL Video Suite.exe"
endlocal
