@echo off
setlocal enabledelayedexpansion

echo ========================================
echo   CCG Gateway Development Environment
echo ========================================
echo.

:: Check required tools
echo [Check] Verifying required tools...
where pnpm > nul 2>&1
if %errorlevel% neq 0 (
    echo [ERROR] pnpm not found. Please install: npm install -g pnpm
    pause
    exit /b 1
)

where cargo > nul 2>&1
if %errorlevel% neq 0 (
    echo [ERROR] cargo not found. Please install Rust: https://rustup.rs/
    pause
    exit /b 1
)
echo [OK] Environment check passed
echo.

:: Install/update frontend dependencies
echo [1/3] Installing frontend dependencies...
cd frontend
pnpm install
if %errorlevel% neq 0 (
    echo [ERROR] Frontend dependencies installation failed
    cd ..
    pause
    exit /b 1
)
cd ..
echo.

:: Start frontend dev server
echo [2/3] Starting frontend dev server...
start "CCG Gateway - Frontend" cmd /k "cd frontend && echo [Frontend] Starting... && pnpm dev"
echo [INFO] Frontend server started in a new window
echo.

:: Start Tauri backend
echo [3/3] Starting Tauri backend...
echo [INFO] First run may take longer to compile dependencies
echo.
cd src-tauri
set RUST_LOG=debug,ccg_gateway=debug,ccg_gateway_lib=debug
cargo run

if %errorlevel% neq 0 (
    echo.
    echo [ERROR] Tauri backend failed to start
    cd ..
    pause
    exit /b 1
)

cd ..
echo.
echo ========================================
echo   Development environment exited
echo ========================================
pause
