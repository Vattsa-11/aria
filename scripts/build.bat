@echo off
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul 2>&1
if errorlevel 1 (
  echo vcvars64.bat not found. Install VS 2022 Build Tools with the VCTools workload.
  exit /b 1
)
cd /d "%~dp0.."
cargo %*
