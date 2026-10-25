# SPDX-License-Identifier: MIT
param([Parameter(Mandatory)][string]$Directory,[Parameter(Mandatory)][string]$Destination)
$ErrorActionPreference='Stop'
Add-Type -AssemblyName System.IO.Compression.FileSystem
if(Test-Path -LiteralPath $Destination){throw 'Output exists'}
[IO.Compression.ZipFile]::CreateFromDirectory((Resolve-Path -LiteralPath $Directory).Path,$Destination,[IO.Compression.CompressionLevel]::Optimal,$true)
