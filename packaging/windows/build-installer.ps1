param(
    [string]$Configuration = "release",
    [string]$MingwRoot = "C:\msys64\mingw64",
    [string]$OutputDir = ""
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

function New-SafeId {
    param(
        [string]$Prefix,
        [string]$Value
    )

    $safe = ($Value -replace '[^A-Za-z0-9_]', '_')
    $safe = ($safe -replace '_+', '_').Trim('_')

    if ([string]::IsNullOrWhiteSpace($safe)) {
        $safe = "Root"
    }

    $full = "$Prefix$safe"
    if ($full.Length -le 72) {
        return $full
    }

    $md5 = [System.Security.Cryptography.MD5]::Create()
    try {
        $hashBytes = $md5.ComputeHash([System.Text.Encoding]::UTF8.GetBytes($full))
    }
    finally {
        $md5.Dispose()
    }
    $hash = ([System.BitConverter]::ToString($hashBytes)).Replace("-", "").ToLowerInvariant().Substring(0, 10)
    $budget = 72 - $Prefix.Length - 1 - $hash.Length
    $trimmed = $safe.Substring(0, [Math]::Max(1, $budget))
    return "$Prefix$trimmed`_$hash"
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

function New-DirectoryTreeXml {
    param(
        [System.Collections.Generic.Dictionary[string, string]]$DirectoryIds,
        [string]$ParentRelativePath
    )

    $children = $DirectoryIds.Keys |
        Where-Object {
            $candidate = $_
            if ($ParentRelativePath -eq "") {
                return $candidate -ne "" -and -not $candidate.Contains('\')
            }

            if (-not $candidate.StartsWith("$ParentRelativePath\")) {
                return $false
            }

            $remainder = $candidate.Substring($ParentRelativePath.Length + 1)
            return -not $remainder.Contains('\')
        } |
        Sort-Object

    $builder = [System.Text.StringBuilder]::new()
    foreach ($child in $children) {
        $name = Split-Path $child -Leaf
        $id = $DirectoryIds[$child]
        [void]$builder.AppendLine("      <Directory Id=""$id"" Name=""$name"">")
        [void]$builder.Append((New-DirectoryTreeXml -DirectoryIds $DirectoryIds -ParentRelativePath $child))
        [void]$builder.AppendLine("      </Directory>")
    }

    return $builder.ToString()
}

function New-ComponentXml {
    param(
        [string]$StageRoot,
        [System.Collections.Generic.Dictionary[string, string]]$DirectoryIds,
        [System.IO.FileInfo[]]$Files
    )

    $groupRefs = [System.Text.StringBuilder]::new()
    $componentXml = [System.Text.StringBuilder]::new()

    foreach ($file in $Files | Sort-Object FullName) {
        $relativePath = Get-RelativePath -BasePath $StageRoot -TargetPath $file.FullName
        $relativeDirectory = Split-Path $relativePath -Parent
        if ($null -eq $relativeDirectory) {
            $relativeDirectory = ""
        }

        $directoryId = if ($relativeDirectory -eq "") { "INSTALLFOLDER" } else { $DirectoryIds[$relativeDirectory] }
        $componentId = New-SafeId -Prefix "cmp_" -Value $relativePath
        $fileId = New-SafeId -Prefix "fil_" -Value $relativePath
        $sourcePath = $file.FullName

        [void]$componentXml.AppendLine("  <Fragment>")
        [void]$componentXml.AppendLine("    <DirectoryRef Id=""$directoryId"">")
        [void]$componentXml.AppendLine("      <Component Id=""$componentId"" Guid=""*"">")
        [void]$componentXml.AppendLine("        <File Id=""$fileId"" Source=""$sourcePath"" KeyPath=""yes"" />")
        [void]$componentXml.AppendLine("      </Component>")
        [void]$componentXml.AppendLine("    </DirectoryRef>")
        [void]$componentXml.AppendLine("  </Fragment>")
        [void]$groupRefs.AppendLine("      <ComponentRef Id=""$componentId"" />")
    }

    return @{
        ComponentGroupRefs = $groupRefs.ToString()
        Components = $componentXml.ToString()
    }
}

$repoRoot = Get-RepoRoot
$cargoTomlPath = Join-Path $repoRoot "Cargo.toml"
$version = Get-PackageVersion -CargoTomlPath $cargoTomlPath
$packageName = "rcommander"
$packageRoot = Join-Path $repoRoot "target\packages\${packageName}_${version}_windows-x64"
$stageRoot = Join-Path $packageRoot "stage"

if ([string]::IsNullOrWhiteSpace($OutputDir)) {
    $OutputDir = $packageRoot
}

Push-Location $repoRoot
try {
    & (Join-Path $PSScriptRoot "stage-runtime.ps1") -Configuration $Configuration -MingwRoot $MingwRoot -StageRoot $stageRoot
    if ($LASTEXITCODE -ne 0) {
        throw "Runtime staging failed."
    }
}
finally {
    Pop-Location
}

$outputRoot = if ([System.IO.Path]::IsPathRooted($OutputDir)) {
    $OutputDir
}
else {
    Join-Path $repoRoot $OutputDir
}
$productWxs = Join-Path $PSScriptRoot "Product.wxs"
$msiPath = Join-Path $outputRoot "RCommander-$version-x64.msi"
$appIconPath = Join-Path $repoRoot "assets\icons\app_icon.ico"

if (-not (Test-Path $appIconPath)) {
    throw "Application icon not found: $appIconPath"
}

New-Item -ItemType Directory -Path $outputRoot -Force | Out-Null

$generatedWxs = Join-Path $outputRoot "GeneratedFiles.wxs"

$files = Get-ChildItem -Path $stageRoot -Recurse -File
$relativeDirectories = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)

foreach ($file in $files) {
    $relativePath = Get-RelativePath -BasePath $stageRoot -TargetPath $file.FullName
    $relativeDirectory = Split-Path $relativePath -Parent

    while (-not [string]::IsNullOrWhiteSpace($relativeDirectory)) {
        $null = $relativeDirectories.Add($relativeDirectory)
        $relativeDirectory = Split-Path $relativeDirectory -Parent
    }
}

$directoryIds = [System.Collections.Generic.Dictionary[string, string]]::new([System.StringComparer]::OrdinalIgnoreCase)
foreach ($relativeDirectory in $relativeDirectories) {
    $directoryIds[$relativeDirectory] = New-SafeId -Prefix "dir_" -Value $relativeDirectory
}

$directoryTree = New-DirectoryTreeXml -DirectoryIds $directoryIds -ParentRelativePath ""
$componentParts = New-ComponentXml -StageRoot $stageRoot -DirectoryIds $directoryIds -Files $files

$generatedContent = @"
<?xml version="1.0" encoding="UTF-8"?>
<Wix xmlns="http://wixtoolset.org/schemas/v4/wxs">
  <Fragment>
    <DirectoryRef Id="INSTALLFOLDER">
$directoryTree    </DirectoryRef>
  </Fragment>

  <Fragment>
    <ComponentGroup Id="StagedFiles">
$($componentParts.ComponentGroupRefs)    </ComponentGroup>
  </Fragment>

$($componentParts.Components)</Wix>
"@

Set-Content -Path $generatedWxs -Value $generatedContent -Encoding utf8

Push-Location $repoRoot
try {
    wix.exe build `
        -arch x64 `
        -o $msiPath `
        -d ProductVersion=$version `
        -d AppIconPath=$appIconPath `
        -src $productWxs `
        -src $generatedWxs

    if ($LASTEXITCODE -ne 0) {
        throw "WiX build failed."
    }
}
finally {
    Pop-Location
}

Write-Host "Built installer: $msiPath"
