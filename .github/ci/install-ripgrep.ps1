# Windows comparison jobs run on x64 GitHub-hosted runners.
$ErrorActionPreference = 'Stop'
$version = '15.2.0'
$name = "ripgrep-$version-x86_64-pc-windows-msvc"
$archive = Join-Path $env:RUNNER_TEMP "$name.zip"
$expected = '71b2fef860abe467217a538ff31de02f5258807c0129f771846f87bd029aafc5'

Invoke-WebRequest -Uri "https://github.com/BurntSushi/ripgrep/releases/download/$version/$name.zip" -OutFile $archive -MaximumRetryCount 3 -RetryIntervalSec 2
if ((Get-FileHash -Algorithm SHA256 -Path $archive).Hash -ne $expected) {
    throw "SHA-256 mismatch for $name.zip"
}
Expand-Archive -LiteralPath $archive -DestinationPath $env:RUNNER_TEMP -Force
$bin = Join-Path $env:RUNNER_TEMP $name
& (Join-Path $bin 'rg.exe') --version
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
Add-Content -LiteralPath $env:GITHUB_PATH -Value $bin -Encoding utf8
