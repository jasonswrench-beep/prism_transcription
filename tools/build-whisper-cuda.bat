@echo off
REM Phase 0 only: builds whisper.cpp's whisper-cli with CUDA, to benchmark the heavy pack (Whisper large-v3-turbo).
REM Not part of the app. Usage: build-whisper-cuda.bat <whisper.cpp source folder> <build folder>
REM Needs Visual Studio's C++ tools, CMake and Ninja, and the CUDA toolkit (CUDA_PATH). Builds only for the
REM graphics card in this computer (CMAKE_CUDA_ARCHITECTURES=native) to keep the build short. CUDA 12.8 does not
REM know Visual Studio 2026 yet, so nvcc is told to allow it (-allow-unsupported-compiler).
setlocal
set "SRC=%~1"
set "OUT=%~2"
if "%SRC%"=="" ( echo usage: %~nx0 ^<source^> ^<build^> & exit /b 2 )
set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
for /f "usebackq delims=" %%i in (`"%VSWHERE%" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do set "VSROOT=%%i"
call "%VSROOT%\VC\Auxiliary\Build\vcvars64.bat" >nul || exit /b 1
set "PATH=%CUDA_PATH%\bin;%PATH%"
cmake -S "%SRC%" -B "%OUT%" -G Ninja -DCMAKE_BUILD_TYPE=Release -DGGML_CUDA=ON -DCMAKE_CUDA_ARCHITECTURES=native -DWHISPER_BUILD_TESTS=OFF -DWHISPER_BUILD_EXAMPLES=ON -DBUILD_SHARED_LIBS=OFF "-DCMAKE_CUDA_FLAGS=-allow-unsupported-compiler -D_ALLOW_COMPILER_AND_STL_VERSION_MISMATCH" || exit /b 1
cmake --build "%OUT%" --target whisper-cli -j 16 || exit /b 1
echo built %OUT%\bin\whisper-cli.exe
