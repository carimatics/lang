param([Parameter(Mandatory)][string]$OutputDirectory)

$ErrorActionPreference = 'Stop'
$version = '23.1.4'
$sha256 = '15e49aca3540991a6f28a0e57babab67da82155c4f73f79dd5e8666a851f9127'
$url = "https://github.com/llvm/llvm-project/releases/download/llvmorg-$version/LLVM-$version-win64.msi"
if (Test-Path $OutputDirectory) { throw 'Debugger output directory must be fresh' }
$null = New-Item -ItemType Directory -Path $OutputDirectory
$archive = Join-Path $OutputDirectory 'llvm.msi'
Invoke-WebRequest -Uri $url -OutFile $archive
if ((Get-Item $archive).Length -ne 639641329 -or
    (Get-FileHash -Algorithm SHA256 $archive).Hash.ToLowerInvariant() -ne $sha256) {
    throw 'Official LLVM MSI size or SHA-256 does not match the pinned release'
}
# Administrative extraction creates a private image without registering an
# installation or replacing the runner's existing LLVM tools.
$image = Join-Path $OutputDirectory 'image'
$log = Join-Path $OutputDirectory 'extract.log'
$extract = Start-Process -FilePath "$env:WINDIR/System32/msiexec.exe" -Wait -PassThru -ArgumentList @(
    '/a', "`"$archive`"", '/qn', "TARGETDIR=`"$image`"", '/l*v', "`"$log`""
)
if ($extract.ExitCode -ne 0) {
    Get-Content $log -Tail 20
    throw "LLVM administrative extraction failed: $($extract.ExitCode)"
}
$adapters = @(Get-ChildItem $OutputDirectory -Recurse -File -Filter 'lldb-dap.exe')
if ($adapters.Count -ne 1) {
    $inventory = @(Get-ChildItem $OutputDirectory -Recurse -File -Filter '*lldb*' |
        Select-Object -First 32 -ExpandProperty FullName)
    throw "Expected exactly one official lldb-dap.exe; found $($adapters.Count); LLDB inventory: $inventory"
}
$adapter = $adapters[0].FullName
$library = Join-Path $adapters[0].DirectoryName 'liblldb.dll'
$inspector = Join-Path $env:ProgramFiles 'LLVM/bin/llvm-readobj.exe'
$imports = & $inspector --coff-imports $library
if ($LASTEXITCODE -ne 0) { throw 'Cannot inspect the actual LLDB Python ABI' }
$pythonDlls = @($imports | ForEach-Object {
    if ($_ -match '^\s*Name:\s+(python([0-9])([0-9]*)\.dll)\s*$') { $Matches[1] }
} | Sort-Object -Unique)
if ($pythonDlls.Count -gt 1) { throw 'Ambiguous LLDB Python ABI' }
"adapter=$adapter" >> $env:GITHUB_OUTPUT
if ($pythonDlls.Count -eq 1) {
    $dll = $pythonDlls[0]
    if ($dll -eq 'python3.dll') {
        # The observed LLVM 23 Windows fixture works with runner Python 3.12.
        'python-version=3.12' >> $env:GITHUB_OUTPUT
    } else {
        $null = $dll -match '^python([0-9])([0-9]+)\.dll$'
        "python-version=$($Matches[1]).$($Matches[2])" >> $env:GITHUB_OUTPUT
    }
    "python-dll=$dll" >> $env:GITHUB_OUTPUT
}
Write-Host "Pinned LLVM $version verified: $sha256; adapter=$adapter; Python=$pythonDlls"
