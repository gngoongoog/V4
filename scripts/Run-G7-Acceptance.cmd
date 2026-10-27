@echo off
setlocal
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0Tools\Run-G7-Acceptance.ps1" %*
set "code=%ERRORLEVEL%"
echo.
if not "%code%"=="0" echo G7-H verification could not complete. Original Core data was not modified.
pause
exit /b %code%
