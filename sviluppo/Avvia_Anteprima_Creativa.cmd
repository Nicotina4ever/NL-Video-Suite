@echo off
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0tools\Avvia_Anteprima_Creativa.ps1"
if errorlevel 1 pause
