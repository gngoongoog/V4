@echo off
setlocal
cd /d "%~dp0"
set "PY="
if exist "%~dp0runtime\python\python.exe" set "PY=%~dp0runtime\python\python.exe"
if not defined PY (
    where py >nul 2>&1
    if not errorlevel 1 set "PY=py -3"
)
if not defined PY (
    echo Python 3 not available. Install Python 3.13 or run from a complete Portable folder.
    pause
    exit /b 2
)
if not exist "%~dp0Tools\v25_verify_system.py" (
    echo Missing Tools\v25_verify_system.py. Extract the V25 P0 diagnostics kit here.
    pause
    exit /b 2
)
%PY% "%~dp0Tools\v25_verify_system.py" --portable "%~dp0" --output "%~dp0verification-output"
set "RC=%ERRORLEVEL%"
echo.
echo Report: verification-output\v25-verification.html
if exist "%~dp0verification-output\v25-verification.html" start "" "%~dp0verification-output\v25-verification.html"
pause
exit /b %RC%
