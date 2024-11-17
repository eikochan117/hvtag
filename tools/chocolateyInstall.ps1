$ErrorActionPreference = 'Stop'

$toolsDir   = "$(Split-Path -parent $MyInvocation.MyCommand.Definition)"
$url        = 'https://github.com/eikochan117/hvtag/releases/download/v1.0.9/hvtag.exe'
$checksumUrl = 'https://github.com/eikochan117/hvtag/releases/download/v1.0.9/hvtag.exe.sha256'

# Récupérer le checksum depuis GitHub
$checksum = (New-Object System.Net.WebClient).DownloadString($checksumUrl).Split('
')[0]

$packageArgs = @{
  packageName   = $env:ChocolateyPackageName
  fileFullPath  = Join-Path $toolsDir 'hvtag.exe'
  url           = $url
  checksum      = $checksum
  checksumType  = 'sha256'
}

Get-ChocolateyWebFile @packageArgs