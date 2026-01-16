@echo off
setlocal

echo === CCG Gateway Build Script ===
echo.

cd /d "%~dp0"

echo [1/4] Building frontend...
cd frontend
call pnpm install
call pnpm build
if errorlevel 1 (
    echo Frontend build failed!
    pause
    exit /b 1
)
cd ..

echo.
echo [2/4] Installing desktop dependencies...
cd backend
uv pip install -e ".[desktop]"
if errorlevel 1 (
    echo Failed to install dependencies!
    pause
    exit /b 1
)
cd ..

echo.
echo [3/4] Running PyInstaller...
cd backend
uv run pyinstaller --noconfirm "..\desktop\ccg-gateway.spec"
if errorlevel 1 (
    echo PyInstaller build failed!
    pause
    exit /b 1
)
cd ..

echo.
echo [4/4] Copying data files...
if not exist "backend\dist\ccg-gateway\data" mkdir "backend\dist\ccg-gateway\data"
if exist ".env" copy ".env" "backend\dist\ccg-gateway\data\.env"
if exist ".env.example" copy ".env.example" "backend\dist\ccg-gateway\data\.env.example"

echo.
echo === Build completed! ===
echo Output: backend\dist\ccg-gateway\
echo.

pause
endlocal
