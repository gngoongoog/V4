@echo off
setlocal
set "ROOT=%~dp0"
set "PYTHON=%ROOT%runtime\python\python.exe"
set "PROBE=%ROOT%Tools\check_mt5_chart_readonly.py"
if not exist "%PYTHON%" (
  1>&2 echo ERROR: bundled Python not found. Extract the complete Windows ZIP.
  exit /b 2
)
if not exist "%PROBE%" (
  1>&2 echo ERROR: read-only MT5 chart probe missing from the portable package.
  exit /b 2
)
1>&2 echo ===========================================================
1>&2 echo  BRAINTRADE V25 - MT5 DEMO CHART CHECK - READ ONLY
1>&2 echo  No orders will be opened, modified, or closed.
1>&2 echo  Open your DEMO account in MT5 before running this check.
1>&2 echo ===========================================================
"%PYTHON%" -B "%PROBE%" %*
set "RESULT=%ERRORLEVEL%"
if not "%RESULT%"=="0" 1>&2 echo Chart check did not pass. Review the JSON status printed above.
if "%~1"=="" pause >CON
exit /b %RESULT%
