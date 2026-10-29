param([Parameter(Mandatory)][string]$OutputDirectory)

$ErrorActionPreference = 'Stop'
$version = '23.1.4'
$sha256 = '1a3f2f998e230b5791cc4e2bfe11a1f7c07fe42c96ff44f227c9db0bedfaf091'
$url = "https://github.com/llvm/llvm-project/releases/download/llvmorg-$version/clang%2Bllvm-$version-x86_64-pc-windows-msvc.tar.zst"
if (Test-Path $OutputDirectory) { throw 'Debugger output directory must be fresh' }
$null = New-Item -ItemType Directory -Path $OutputDirectory
$archive = Join-Path $OutputDirectory 'llvm.tar.zst'
Invoke-WebRequest -Uri $url -OutFile $archive
if ((Get-Item $archive).Length -ne 276626313 -or
    (Get-FileHash -Algorithm SHA256 $archive).Hash.ToLowerInvariant() -ne $sha256) {
    throw 'Official LLVM archive size or SHA-256 does not match the pinned release'
}
$tar = Join-Path $OutputDirectory 'llvm.tar'
& zstd -d $archive -o $tar
if ($LASTEXITCODE -ne 0) { throw 'LLVM archive decompression failed' }
& tar -xf $tar -C $OutputDirectory
if ($LASTEXITCODE -ne 0) { throw 'LLVM archive extraction failed' }
$adapters = @(Get-ChildItem $OutputDirectory -Recurse -File -Filter 'lldb-dap.exe')
if ($adapters.Count -ne 1) { throw 'Expected exactly one official lldb-dap.exe' }
$adapter = $adapters[0].FullName
$library = Join-Path $adapters[0].DirectoryName 'liblldb.dll'
$inspector = Join-Path $env:ProgramFiles 'LLVM/bin/llvm-readobj.exe'
$imports = & $inspector --coff-imports $library
if ($LASTEXITCODE -ne 0) { throw 'Cannot inspect the actual LLDB Python ABI' }
$pythonDlls = @($imports | ForEach-Object {
    if ($_ -match '^\s*Name:\s+(python([0-9])([0-9]+)\.dll)\s*$') { $Matches[1] }
} | Sort-Object -Unique)
if ($pythonDlls.Count -gt 1) { throw 'Ambiguous LLDB Python ABI' }
"adapter=$adapter" >> $env:GITHUB_OUTPUT
if ($pythonDlls.Count -eq 1) {
    $dll = $pythonDlls[0]
    $null = $dll -match '^python([0-9])([0-9]+)\.dll$'
    "python-version=$($Matches[1]).$($Matches[2])" >> $env:GITHUB_OUTPUT
    "python-dll=$dll" >> $env:GITHUB_OUTPUT
}
Write-Host "Pinned LLVM $version verified: $sha256; adapter=$adapter; Python=$pythonDlls"
