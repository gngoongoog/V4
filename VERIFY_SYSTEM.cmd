@echo off
setlocal
cd /d "%~dp0"
if not exist "%~dp0runtime\python\python.exe" (
 echo Extract the complete Portable folder first.
 pause
 exit /b 2
)
"%~dp0runtime\python\python.exe" "%~dp0Tools\v25_verify_system.py" --portable "%~dp0." --output "%~dp0verification-output"
set "RC=%ERRORLEVEL%"
if /I "%~1"=="--ci" exit /b %RC%
if exist "%~dp0verification-output\v25-verification.html" start "" "%~dp0verification-output\v25-verification.html"
pause
exit /b %RC%
