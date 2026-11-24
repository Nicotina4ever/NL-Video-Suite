@echo off
rem Installa per-utente la versione NUOVA di NL Video Suite (nessun admin, nessun servizio AI).
rem Copia il bundle piu recente in %LOCALAPPDATA%\Programs\NL Video Suite e crea le scorciatoie
rem Start Menu / Desktop, cosi l'app si trova anche cercando "NL Video Suite".
setlocal enabledelayedexpansion
set "ROOT=%~dp0"
set "APP="
for /f "delims=" %%D in ('dir /b /ad /o-n "%ROOT%distribuzione\NLVideoSuite-*-windows-x64" 2^>nul') do (
    if not defined APP set "APP=%ROOT%distribuzione\%%D"
)
if not defined APP (
    echo.
    echo Nessuna build trovata in "%ROOT%distribuzione".
    echo Crea prima il portabile con packaging\nl\portable.ps1 oppure chiedi la build aggiornata.
    echo.
    pause
    exit /b 1
)
powershell -NoProfile -ExecutionPolicy Bypass -File "%ROOT%packaging\nl\install.ps1" -Bundle "%APP%" -DesktopShortcut
if errorlevel 1 (
    echo.
    echo Installazione non riuscita.
    pause
    exit /b 1
)
echo.
echo Installazione completata: cerca "NL Video Suite" nel menu Start o sul Desktop.
pause
endlocal
