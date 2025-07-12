$ErrorActionPreference = 'Stop'

# Fetch latest release version
$LatestRelease = Invoke-RestMethod -Uri "https://api.github.com/repos/rotkonetworks/githem/releases/latest"
$Version = $LatestRelease.tag_name -replace '^v', ''

$BaseUrl = "https://github.com/rotkonetworks/githem/releases/download/v$Version"
$Binary = "githem-windows-x64.exe"
$InstallDir = "$env:LOCALAPPDATA\githem\bin"

Write-Host "Installing githem v$Version..." -ForegroundColor Green

# Create install directory
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null

# Download
$Url = "$BaseUrl/$Binary"
$Dest = "$InstallDir\githem.exe"

Write-Host "Downloading from $Url..."
Invoke-WebRequest -Uri $Url -OutFile $Dest -UseBasicParsing

# Verify if possible
$Sha512Url = "$Url.sha512"
try {
    $HashContent = (Invoke-WebRequest -Uri $Sha512Url -UseBasicParsing).Content
    $ExpectedHash = ($HashContent -split '\s+')[0].ToUpper()
    $ActualHash = (Get-FileHash -Path $Dest -Algorithm SHA512).Hash
    
    if ($ExpectedHash -eq $ActualHash) {
        Write-Host "Checksum verified" -ForegroundColor Green
    } else {
        Write-Warning "Checksum verification failed"
    }
} catch {
    Write-Host "Skipping checksum verification"
}

# Add to PATH
$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($UserPath -notlike "*$InstallDir*") {
    [Environment]::SetEnvironmentVariable("Path", "$UserPath;$InstallDir", "User")
    $env:Path = "$env:Path;$InstallDir"
    Write-Host "Added to PATH. You may need to restart your terminal." -ForegroundColor Yellow
}

Write-Host "Installation complete. Run 'githem --version' to verify." -ForegroundColor Green
