@echo off
cd /d "%~dp0"
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\Build-V25-Portable-Windows.ps1"
exit /b %errorlevel%
