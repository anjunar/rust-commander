param(
    [string]$Configuration = "release",
    [string]$StageRoot = "",
    [string]$MingwRoot = "C:\msys64\mingw64"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Get-RepoRoot {
    return (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
}

function Get-PackageVersion {
    param([string]$CargoTomlPath)

    $match = Select-String -Path $CargoTomlPath -Pattern '^\s*version\s*=\s*"(?<version>\d+\.\d+\.\d+)"' |
        Select-Object -First 1

    if (-not $match) {
        throw "Could not determine package version from $CargoTomlPath."
    }

    return $match.Matches[0].Groups["version"].Value
}

function Get-RelativePath {
    param(
        [string]$BasePath,
        [string]$TargetPath
    )

    $baseUri = [Uri]((Resolve-Path $BasePath).Path.TrimEnd('\') + '\')
    $targetUri = [Uri](Resolve-Path $TargetPath).Path
    return [Uri]::UnescapeDataString($baseUri.MakeRelativeUri($targetUri).ToString()).Replace('/', '\')
}

function Copy-DirectoryContent {
    param(
        [string]$SourcePath,
        [string]$DestinationPath
    )

    if (-not (Test-Path $SourcePath)) {
        throw "Required path not found: $SourcePath"
    }

    New-Item -ItemType Directory -Path $DestinationPath -Force | Out-Null
    Copy-Item -Path (Join-Path $SourcePath "*") -Destination $DestinationPath -Recurse -Force
}

function Copy-DirectoryContentIfPresent {
    param(
        [string]$SourcePath,
        [string]$DestinationPath
    )

    if (Test-Path $SourcePath) {
        Copy-DirectoryContent -SourcePath $SourcePath -DestinationPath $DestinationPath
    }
}

function Get-PeDependencies {
    param([string]$BinaryPath)

    $lines = & objdump -p $BinaryPath 2>$null
    if ($LASTEXITCODE -ne 0) {
        throw "objdump failed for $BinaryPath"
    }

    return $lines |
        Select-String 'DLL Name:\s+(.+)$' |
        ForEach-Object { $_.Matches[0].Groups[1].Value.Trim() } |
        Sort-Object -Unique
}

function Copy-RuntimeClosure {
    param(
        [string]$EntryBinary,
        [string]$BinSource,
        [string]$BinDestination
    )

    $ignoredPrefixes = @(
        "api-ms-win-",
        "ext-ms-"
    )

    $ignoredNames = @(
        "advapi32.dll",
        "bcrypt.dll",
        "bcryptprimitives.dll",
        "comctl32.dll",
        "comdlg32.dll",
        "gdi32.dll",
        "gdiplus.dll",
        "imm32.dll",
        "kernel32.dll",
        "msimg32.dll",
        "msvcrt.dll",
        "ntdll.dll",
        "ole32.dll",
        "rpcrt4.dll",
        "setupapi.dll",
        "shell32.dll",
        "shlwapi.dll",
        "ucrtbase.dll",
        "user32.dll",
        "userenv.dll",
        "usp10.dll",
        "win32u.dll",
        "ws2_32.dll"
    )

    $copied = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
    $queue = [System.Collections.Generic.Queue[string]]::new()
    $systemRoots = @(
        (Join-Path $env:WINDIR "System32"),
        (Join-Path $env:WINDIR "SysWOW64")
    )
    $queue.Enqueue((Resolve-Path $EntryBinary).Path)

    while ($queue.Count -gt 0) {
        $current = $queue.Dequeue()
        foreach ($dependency in Get-PeDependencies -BinaryPath $current) {
            $dependencyLower = $dependency.ToLowerInvariant()

            if ($ignoredNames -contains $dependencyLower) {
                continue
            }

            if ($ignoredPrefixes | Where-Object { $dependencyLower.StartsWith($_) }) {
                continue
            }

            if (-not $copied.Add($dependency)) {
                continue
            }

            $sourceDll = Join-Path $BinSource $dependency
            if (-not (Test-Path $sourceDll)) {
                $isSystemDll = $systemRoots |
                    Where-Object { Test-Path (Join-Path $_ $dependency) } |
                    Select-Object -First 1

                if ($isSystemDll) {
                    continue
                }

                throw "Missing runtime dependency $dependency in $BinSource"
            }

            $destinationDll = Join-Path $BinDestination $dependency
            Copy-Item -LiteralPath $sourceDll -Destination $destinationDll -Force
            $queue.Enqueue((Resolve-Path $destinationDll).Path)
        }
    }
}

$repoRoot = Get-RepoRoot
$cargoTomlPath = Join-Path $repoRoot "Cargo.toml"
$version = Get-PackageVersion -CargoTomlPath $cargoTomlPath
$packageName = "rust-commander"
$packageRoot = Join-Path $repoRoot "target\packages\${packageName}_${version}_windows-x64"

if ([string]::IsNullOrWhiteSpace($StageRoot)) {
    $StageRoot = Join-Path $packageRoot "stage"
}

$targetRoot = Join-Path $repoRoot "target\$Configuration"
$exePath = Join-Path $targetRoot "rust-commander.exe"
$stagePath = if ([System.IO.Path]::IsPathRooted($StageRoot)) {
    $StageRoot
}
else {
    Join-Path $repoRoot $StageRoot
}
$stageBinPath = Join-Path $stagePath "bin"

$mingwBin = Join-Path $MingwRoot "bin"
$mingwLib = Join-Path $MingwRoot "lib"
$mingwShare = Join-Path $MingwRoot "share"
$mingwEtc = Join-Path $MingwRoot "etc"

Push-Location $repoRoot
try {
    if ($Configuration -eq "release") {
        cargo build --release
        if ($LASTEXITCODE -ne 0) {
            throw "cargo build --release failed."
        }
    }
    else {
        cargo build --profile $Configuration
        if ($LASTEXITCODE -ne 0) {
            throw "cargo build --profile $Configuration failed."
        }
    }
}
finally {
    Pop-Location
}

if (Test-Path $stagePath) {
    Remove-Item -LiteralPath $stagePath -Recurse -Force
}

New-Item -ItemType Directory -Path $stageBinPath -Force | Out-Null

Copy-Item -LiteralPath $exePath -Destination (Join-Path $stageBinPath "rust-commander.exe") -Force
Copy-Item -LiteralPath (Join-Path $mingwBin "gdbus.exe") -Destination $stageBinPath -Force
Copy-Item -LiteralPath (Join-Path $mingwBin "gspawn-win64-helper.exe") -Destination $stageBinPath -Force
Copy-Item -LiteralPath (Join-Path $mingwBin "gspawn-win64-helper-console.exe") -Destination $stageBinPath -Force

Copy-RuntimeClosure -EntryBinary (Join-Path $stageBinPath "rust-commander.exe") -BinSource $mingwBin -BinDestination $stageBinPath
Copy-RuntimeClosure -EntryBinary (Join-Path $stageBinPath "gdbus.exe") -BinSource $mingwBin -BinDestination $stageBinPath
Copy-RuntimeClosure -EntryBinary (Join-Path $stageBinPath "gspawn-win64-helper.exe") -BinSource $mingwBin -BinDestination $stageBinPath
Copy-RuntimeClosure -EntryBinary (Join-Path $stageBinPath "gspawn-win64-helper-console.exe") -BinSource $mingwBin -BinDestination $stageBinPath

Copy-DirectoryContent -SourcePath (Join-Path $repoRoot "assets") -DestinationPath (Join-Path $stagePath "assets")
Copy-DirectoryContent -SourcePath (Join-Path $mingwLib "gdk-pixbuf-2.0") -DestinationPath (Join-Path $stagePath "lib\gdk-pixbuf-2.0")
Copy-DirectoryContent -SourcePath (Join-Path $mingwLib "gio\modules") -DestinationPath (Join-Path $stagePath "lib\gio\modules")
Copy-DirectoryContent -SourcePath (Join-Path $mingwShare "glib-2.0\schemas") -DestinationPath (Join-Path $stagePath "share\glib-2.0\schemas")
Copy-DirectoryContent -SourcePath (Join-Path $mingwShare "gtk-4.0") -DestinationPath (Join-Path $stagePath "share\gtk-4.0")
Copy-DirectoryContent -SourcePath (Join-Path $mingwShare "icons\Adwaita") -DestinationPath (Join-Path $stagePath "share\icons\Adwaita")
Copy-DirectoryContentIfPresent -SourcePath (Join-Path $mingwShare "icons\AdwaitaLegacy") -DestinationPath (Join-Path $stagePath "share\icons\AdwaitaLegacy")
Copy-DirectoryContent -SourcePath (Join-Path $mingwShare "icons\hicolor") -DestinationPath (Join-Path $stagePath "share\icons\hicolor")
Copy-DirectoryContent -SourcePath (Join-Path $mingwEtc "fonts") -DestinationPath (Join-Path $stagePath "etc\fonts")
Copy-DirectoryContent -SourcePath (Join-Path $mingwShare "fontconfig") -DestinationPath (Join-Path $stagePath "share\fontconfig")

$manifest = [ordered]@{
    ProductName = "RCommander"
    Version = $version
    StageRoot = (Resolve-Path $stagePath).Path
    Executable = "bin\rust-commander.exe"
    GeneratedAtUtc = [DateTime]::UtcNow.ToString("o")
    MingwRoot = $MingwRoot
}

$manifestPath = Join-Path $stagePath "stage-manifest.json"
$manifest | ConvertTo-Json | Set-Content -Path $manifestPath -Encoding utf8

Write-Host "Staged RCommander runtime in $stagePath"
