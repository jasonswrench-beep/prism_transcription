@echo off
REM Builds the heavy pack's program: whisper.cpp's whisper-cli (MIT), run by Prism Transcribe as a separate program.
REM Usage: build-whisper.bat <whisper.cpp source folder> <output folder> [vulkan]
REM
REM The source is whisper.cpp v1.9.5, commit d1be6fde11ac6e0407606b4e42fe72d34add8037 (the same build OpenStage Author
REM uses), from https://github.com/ggml-org/whisper.cpp, for example:
REM   git clone --depth 1 --branch v1.9.5 https://github.com/ggml-org/whisper.cpp.git C:\src\whisper.cpp
REM
REM Without "vulkan" it is the processor build: one program that picks the fastest code for the processor it runs on
REM (GGML_CPU_ALL_VARIANTS, so it works on any x64 computer). With "vulkan" it also uses the graphics card; that needs
REM the Vulkan SDK (VULKAN_SDK set). Copy everything in <output folder> into a "whisper" folder beside
REM prism-transcribe.exe, or point Settings at whisper-cli.exe.
REM Needs Visual Studio's C++ tools with CMake and Ninja ("C++ CMake tools for Windows").
setlocal
set "SRC=%~1"
set "OUT=%~2"
if "%OUT%"=="" ( echo usage: %~nx0 ^<source^> ^<output^> [vulkan] & exit /b 2 )
set "GPU=-DGGML_VULKAN=OFF"
if /i "%~3"=="vulkan" set "GPU=-DGGML_VULKAN=ON"
set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
for /f "usebackq delims=" %%i in (`"%VSWHERE%" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do set "VSROOT=%%i"
if not defined VSROOT ( echo Visual Studio's C++ tools were not found. & exit /b 1 )
call "%VSROOT%\VC\Auxiliary\Build\vcvars64.bat" >nul || exit /b 1
set "VSCMAKE=%VSROOT%\Common7\IDE\CommonExtensions\Microsoft\CMake"
set "PATH=%VSCMAKE%\CMake\bin;%VSCMAKE%\Ninja;%PATH%"
set "BUILD=%OUT%\build"
cmake -S "%SRC%" -B "%BUILD%" -G Ninja -DCMAKE_BUILD_TYPE=Release -DBUILD_SHARED_LIBS=ON -DGGML_NATIVE=OFF -DGGML_BACKEND_DL=ON -DGGML_CPU_ALL_VARIANTS=ON -DGGML_OPENMP=OFF -DWHISPER_BUILD_TESTS=OFF -DWHISPER_BUILD_EXAMPLES=ON -DWHISPER_SDL2=OFF %GPU% || exit /b 1
cmake --build "%BUILD%" --target whisper-cli -j 16 || exit /b 1
copy /y "%BUILD%\bin\*.exe" "%OUT%\" >nul
copy /y "%BUILD%\bin\*.dll" "%OUT%\" >nul
rmdir /s /q "%BUILD%"
echo Built %OUT%\whisper-cli.exe
