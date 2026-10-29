@echo off
setlocal
where node >nul 2>nul
if errorlevel 1 (
  echo Node.js is missing. Install the version specified in .node-version, then retry.
  pause
  exit /b 1
)
node "%~dp0scripts\start-web.mjs" --open
if errorlevel 1 (
  pause
  exit /b 1
)
