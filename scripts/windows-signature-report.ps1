# SPDX-License-Identifier: MIT
param([Parameter(Mandatory)][string]$Path)
$ErrorActionPreference='Stop'
[Console]::OutputEncoding=[Text.UTF8Encoding]::new($false)
$taskSignature=Get-AuthenticodeSignature -LiteralPath (Resolve-Path -LiteralPath $Path).Path
[ordered]@{
  status=$taskSignature.Status.ToString()
  trusted=($taskSignature.Status -eq 'Valid')
  subject=$(if($taskSignature.SignerCertificate){$taskSignature.SignerCertificate.Subject}else{$null})
  thumbprint=$(if($taskSignature.SignerCertificate){$taskSignature.SignerCertificate.Thumbprint}else{$null})
} | ConvertTo-Json -Compress
