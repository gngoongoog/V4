@echo off
setlocal
set "ROOT=%~dp0"
set "PYTHON=%ROOT%runtime\python\python.exe"
set "PROBE=%ROOT%Tools\check_mt5_chart_readonly.py"
if not exist "%PYTHON%" (
  echo ERROR: bundled Python not found. Extract the complete Windows ZIP.
  exit /b 2
)
if not exist "%PROBE%" (
  echo ERROR: read-only MT5 chart probe missing from the portable package.
  exit /b 2
)
echo ===========================================================
echo  BRAINTRADE V25 - MT5 DEMO CHART CHECK - READ ONLY
echo  No orders will be opened, modified, or closed.
echo  Open your DEMO account in MT5 before running this check.
echo ===========================================================
"%PYTHON%" -B "%PROBE%" %*
set "RESULT=%ERRORLEVEL%"
echo.
if not "%RESULT%"=="0" echo Chart check did not pass. Review the JSON status printed above.
exit /b %RESULT%
