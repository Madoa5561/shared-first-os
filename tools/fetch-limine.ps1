# Downloads only the locked boot files into this project's .tools directory.
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$lock = Get-Content -Raw -LiteralPath "$PSScriptRoot/versions.json" | ConvertFrom-Json
$destination = Join-Path $root '.tools/limine'
New-Item -ItemType Directory -Force -Path $destination | Out-Null
foreach ($entry in $lock.limine.files.PSObject.Properties) {
    $path = Join-Path $destination $entry.Name
    if (-not (Test-Path -LiteralPath $path)) {
        $url = "https://raw.githubusercontent.com/limine-bootloader/limine/$($lock.limine.commit)/$($entry.Name)"
        Write-Host "Download $url"
        Invoke-WebRequest -Uri $url -OutFile $path
    }
    $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash.ToLowerInvariant()
    if ($actual -ne $entry.Value) { throw "Limine hash mismatch: $path" }
    Write-Host "$($entry.Name): SHA256 $actual verified"
}
