$ErrorActionPreference = 'Stop'

$toolsDir = "$(Split-Path -parent $MyInvocation.MyCommand.Definition)"
$fileToRemove = Join-Path $toolsDir 'hvtag.exe'

# Supprimer le fichier .exe
Remove-Item $fileToRemove -Force -ErrorAction SilentlyContinue