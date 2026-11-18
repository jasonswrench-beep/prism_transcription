# Builds the interface, the desktop app and the Windows installer, then writes the installer's SHA-256 beside it.
# Adapted from Prism Survey's app\BUILD-INSTALLER.ps1.
#
#   powershell -ExecutionPolicy Bypass -File app\BUILD-INSTALLER.ps1                         an unsigned installer
#   powershell -ExecutionPolicy Bypass -File app\BUILD-INSTALLER.ps1 -Sign                   signed and timestamped
#   powershell -ExecutionPolicy Bypass -File app\BUILD-INSTALLER.ps1 -Whisper <folder> ...   with the heavy pack's program
#
# BUILD PRISM TRANSCRIBE INSTALLER.bat in the project folder runs the first form.
#
# It builds only from a tagged commit with nothing uncommitted (CLAUDE.md, "Releases"), so each release tag points at the
# exact commit its build came from; it stops otherwise. The result is target\release\bundle\nsis\
# Prism Transcribe_<version>_x64-setup.exe, with a .sha256.txt and a .source.txt (the tag and commit) beside it.
#
# -Whisper names a folder holding whisper.cpp's whisper-cli.exe and its DLLs (tools\build-whisper.bat makes one). They
# go into the installer's "whisper" folder, where the app looks first, so the heavy pack works without setup. Without
# it, the installer has no whisper-cli and Settings asks for one when the heavy pack is used.
#
# -Sign uses the suite's signing scripts rather than a copy of them. OPENSTAGE_SIGN_TOOLS names the folder that holds
# sign_windows.py and check_signatures.py (as in the PrismQDA and OpenStage repositories); their header says which
# settings and sign-in they need (OPENSTAGE_SIGN_* and az login). Tauri is given sign_windows.py as its signCommand
# through a temporary configuration, so no signing setting goes into tauri.conf.json or the repository. whisper-cli and
# its DLLs are signed before they go in. check_signatures.py then checks the setup and every program inside it before
# the SHA-256 is written, so the hash is the signed file's. The build stops if anything is unsigned or untimestamped.

param([switch]$Sign, [string]$Whisper)

$ErrorActionPreference = 'Stop'
if ([Environment]::ProcessorCount -ge 32) {
    # The sibling builds' rule: CPUs 16 to 31, the efficiency cores, because long builds are unstable on the others.
    [System.Diagnostics.Process]::GetCurrentProcess().ProcessorAffinity = [IntPtr]4294901760
}

$app = Split-Path -Parent $MyInvocation.MyCommand.Path
$root = Split-Path -Parent $app

Push-Location $root
$pending = git status --porcelain
if ($LASTEXITCODE) { Pop-Location; throw 'git is needed to check what the build is made from' }
if ($pending) { Pop-Location; throw "There are changes not committed. Commit everything and tag the commit before building:`n$($pending -join "`n")" }
$tag = git describe --exact-match --tags HEAD 2>$null
if ($LASTEXITCODE -or -not $tag) { Pop-Location; throw 'This commit has no tag. Tag it (for example v0.1.0-alpha.1) before building, so the release tag points at this build.' }
$commit = git rev-parse HEAD
$version = (Get-Content (Join-Path $app 'src-tauri\tauri.conf.json') -Raw | ConvertFrom-Json).version
if ($tag -ne "v$version") { Pop-Location; throw "The tag is $tag but the app's version is $version; they must match (v$version)." }
Pop-Location
"Building Prism Transcribe $version from tag $tag (commit $commit)"

# Third-party licences: THIRD-PARTY.md must be current and every licence accepted.
python (Join-Path $root 'tools\third_party.py')
if ($LASTEXITCODE) { throw 'tools\third_party.py found a licence that is not accepted (see above)' }
Push-Location $root
$changed = git status --porcelain THIRD-PARTY.md
Pop-Location
if ($changed) { throw 'THIRD-PARTY.md changed: commit it, tag again, and build again' }

# Leaves the builder's folders out of the program (Rust writes source paths into error messages).
$remap = @("--remap-path-prefix=$env:USERPROFILE=/user", "--remap-path-prefix=$root=/prismtranscribe")
$env:CARGO_ENCODED_RUSTFLAGS = (@($env:CARGO_ENCODED_RUSTFLAGS) + $remap | Where-Object { $_ }) -join [string][char]31

$dryRun = ($env:OPENSTAGE_SIGN_DRY_RUN -eq '1')
$python = (Get-Command python -ErrorAction SilentlyContinue).Source
if ($Sign) {
    $tools = $env:OPENSTAGE_SIGN_TOOLS
    if (-not $tools -or -not (Test-Path (Join-Path $tools 'sign_windows.py'))) {
        throw 'OPENSTAGE_SIGN_TOOLS must name the folder that holds the suite''s sign_windows.py and check_signatures.py'
    }
    if (-not $python) { throw 'python was not found, and signing needs it' }
    if (-not $env:OPENSTAGE_SIGN_PUBLISHER) { throw 'OPENSTAGE_SIGN_PUBLISHER is not set: it names the signer the build must see on every file' }
    if (-not $dryRun) {
        & $python (Join-Path $tools 'sign_windows.py') --ready
        if ($LASTEXITCODE) { throw 'signing cannot start: see what is missing above' }
    }
}

$work = Join-Path $root 'target\installer-work'
New-Item -ItemType Directory -Force $work | Out-Null
$config = @{ bundle = @{ windows = @{} } }
if ($Sign) {
    $config.bundle.windows.signCommand = @{ cmd = $python; args = @((Join-Path $env:OPENSTAGE_SIGN_TOOLS 'sign_windows.py'), '--description', 'Prism Transcribe', '%1') }
}
if ($Whisper) {
    if (-not (Test-Path (Join-Path $Whisper 'whisper-cli.exe'))) { throw "$Whisper has no whisper-cli.exe" }
    $stage = Join-Path $work 'whisper'
    if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
    New-Item -ItemType Directory -Force $stage | Out-Null
    Copy-Item (Join-Path $Whisper '*.exe'), (Join-Path $Whisper '*.dll') $stage
    if ($Sign -and -not $dryRun) {
        foreach ($f in Get-ChildItem $stage -File) {
            & $python (Join-Path $env:OPENSTAGE_SIGN_TOOLS 'sign_windows.py') --description 'whisper.cpp (for Prism Transcribe)' $f.FullName
            if ($LASTEXITCODE) { throw "could not sign $($f.Name)" }
        }
    }
    $res = [ordered]@{ '../../LICENSE' = 'LICENSE'; '../../NOTICE' = 'NOTICE'; '../../THIRD-PARTY.md' = 'THIRD-PARTY.md' }
    foreach ($f in Get-ChildItem $stage -File) { $res[$f.FullName.Replace('\', '/')] = "whisper/$($f.Name)" }
    $config.bundle.resources = $res
}
$configFile = Join-Path $work 'tauri.release.conf.json'
$config | ConvertTo-Json -Depth 8 | Out-File -Encoding ascii $configFile

Set-Location (Join-Path $app 'ui')
if (-not (Test-Path 'node_modules')) { npm ci --no-audit --no-fund; if ($LASTEXITCODE) { throw 'npm ci failed' } }
npm run build
if ($LASTEXITCODE) { throw 'the interface did not build' }

Set-Location $app
node ui\node_modules\@tauri-apps\cli\tauri.js build --ci --config $configFile
if ($LASTEXITCODE) { throw 'the app did not build' }

$installer = Get-ChildItem (Join-Path $root 'target\release\bundle\nsis') -Filter '*-setup.exe' | Sort-Object LastWriteTime -Descending | Select-Object -First 1
if ($Sign -and -not $dryRun) {
    & $python (Join-Path $env:OPENSTAGE_SIGN_TOOLS 'check_signatures.py') $installer.FullName
    if ($LASTEXITCODE) { throw 'the setup was built, but it is not signed as it should be (see above)' }
}
$hash = (Get-FileHash $installer.FullName -Algorithm SHA256).Hash.ToLower()
"$hash  $($installer.Name)" | Out-File -Encoding ascii "$($installer.FullName).sha256.txt"
"Built from tag $tag, commit $commit" | Out-File -Encoding ascii "$($installer.FullName).source.txt"
"Built $($installer.FullName) from tag $tag"
"SHA-256 $hash"
