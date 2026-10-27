@echo off
setlocal
title BRAINTRADE V25 - Read-only G6 Demo Study
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0Tools\Run-G6-Study.ps1" %*
set "G6_EXIT=%ERRORLEVEL%"
if not "%G6_EXIT%"=="0" (
  echo.
  echo Study was NOT verified. Review the error above. No trade was sent.
)
echo.
pause
exit /b %G6_EXIT%
