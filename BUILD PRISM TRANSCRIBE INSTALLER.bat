@echo off
REM Builds an unsigned Prism Transcribe installer from the tagged commit (see app\BUILD-INSTALLER.ps1 for signing and
REM for putting whisper-cli inside). Stops if anything is uncommitted or the commit has no tag.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0app\BUILD-INSTALLER.ps1" %*
if errorlevel 1 (
  echo.
  echo The installer was not built: see the message above.
  pause
  exit /b 1
)
pause
