#Requires -Version 5.1
<#
.SYNOPSIS
    github-notifier-ws Windows インストーラー

.DESCRIPTION
    GitHub Packages (ghcr.io) から OCI アーティファクトとして配布されている
    github-notifier-ws の NSIS インストーラーをダウンロードして実行します。

.PARAMETER Version
    インストールするバージョン（例: v0.1.0）。省略時は latest。

.PARAMETER Owner
    GitHub ユーザー / Org 名。省略時は "coxless"。

.PARAMETER Silent
    NSIS の /S フラグを使いサイレントインストールします。

.EXAMPLE
    # 最新版をインストール
    irm https://raw.githubusercontent.com/coxless/github-notifier-ws/main/install.ps1 | iex

.EXAMPLE
    # バージョン指定
    & ([scriptblock]::Create((irm https://raw.githubusercontent.com/coxless/github-notifier-ws/main/install.ps1))) -Version v0.1.0
#>
[CmdletBinding()]
param(
    [string]$Version = "latest",
    [string]$Owner   = "coxless",
    [switch]$Silent
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$ProgressPreference    = "SilentlyContinue"  # faster Invoke-WebRequest

$package = "$Owner/github-notifier-ws"
$registry = "ghcr.io"

function Write-Step([string]$msg) { Write-Host "  $msg" -ForegroundColor Cyan }
function Write-Ok([string]$msg)   { Write-Host "  OK  $msg" -ForegroundColor Green }
function Abort([string]$msg)      { Write-Host "ERROR: $msg" -ForegroundColor Red; exit 1 }

Write-Host ""
Write-Host "github-notifier-ws インストーラー" -ForegroundColor White
Write-Host "パッケージ: $registry/$package`:$Version"
Write-Host ""

# ── 1. Anonymous token for public ghcr.io package ────────────────────────────
Write-Step "GitHub Container Registry のトークンを取得中..."
try {
    $tokenUri = "https://ghcr.io/token?scope=repository:${package}:pull&service=ghcr.io"
    $tok = (Invoke-RestMethod -Uri $tokenUri -Method GET).token
} catch {
    Abort "トークン取得失敗: $_"
}
$authHeader = @{ "Authorization" = "Bearer $tok" }
Write-Ok "トークン取得"

# ── 2. Resolve manifest (handles both image index and manifest) ───────────────
Write-Step "マニフェストを取得中 ($Version)..."
$manifestUri = "https://ghcr.io/v2/$package/manifests/$Version"
$manifestHeaders = $authHeader + @{
    "Accept" = "application/vnd.oci.image.manifest.v1+json,application/vnd.oci.image.index.v1+json,application/vnd.docker.distribution.manifest.v2+json"
}
try {
    $manifest = Invoke-RestMethod -Uri $manifestUri -Headers $manifestHeaders -Method GET
} catch {
    Abort "マニフェスト取得失敗 ($Version): $_"
}

# If index, pick the first manifest entry and re-fetch
if ($manifest.manifests) {
    $childDigest = $manifest.manifests[0].digest
    Write-Step "  → インデックスから $childDigest を選択"
    $manifest = Invoke-RestMethod -Uri "https://ghcr.io/v2/$package/manifests/$childDigest" `
        -Headers $manifestHeaders -Method GET
}

# Find the installer layer (application/octet-stream)
$layer = $manifest.layers | Where-Object { $_.mediaType -eq "application/octet-stream" } | Select-Object -First 1
if (-not $layer) {
    # Fallback: take first layer
    $layer = $manifest.layers | Select-Object -First 1
}
if (-not $layer) { Abort "インストーラーレイヤーが見つかりません" }

$digest    = $layer.digest
$title     = if ($layer.annotations."org.opencontainers.image.title") { $layer.annotations."org.opencontainers.image.title" } else { "github-notifier-ws-setup.exe" }
Write-Ok "マニフェスト取得 ($title, $([math]::Round($layer.size/1MB, 1)) MB)"

# ── 3. Download installer blob ───────────────────────────────────────────────
$tmpDir  = Join-Path $env:TEMP "gnws-install-$(Get-Random)"
New-Item -ItemType Directory -Path $tmpDir -Force | Out-Null
$outFile = Join-Path $tmpDir $title

Write-Step "ダウンロード中 ($title)..."
try {
    $blobUri = "https://ghcr.io/v2/$package/blobs/$digest"
    Invoke-WebRequest -Uri $blobUri -Headers $authHeader -OutFile $outFile
} catch {
    Remove-Item -Recurse -Force $tmpDir -ErrorAction SilentlyContinue
    Abort "ダウンロード失敗: $_"
}
Write-Ok "ダウンロード完了: $outFile"

# ── 4. Verify the download ───────────────────────────────────────────────────
$expectedSize = $layer.size
$actualSize   = (Get-Item $outFile).Length
if ($actualSize -ne $expectedSize) {
    Remove-Item -Recurse -Force $tmpDir -ErrorAction SilentlyContinue
    Abort "サイズ不一致 (expected $expectedSize, got $actualSize)"
}

# ── 5. Run installer ─────────────────────────────────────────────────────────
Write-Step "インストール中..."
$installArgs = if ($Silent) { @("/S") } else { @() }
try {
    $proc = Start-Process -FilePath $outFile -ArgumentList $installArgs -PassThru -Wait
    if ($proc.ExitCode -ne 0) {
        Abort "インストーラーが終了コード $($proc.ExitCode) で終了しました"
    }
} catch {
    Abort "インストーラー起動失敗: $_"
} finally {
    Remove-Item -Recurse -Force $tmpDir -ErrorAction SilentlyContinue
}

Write-Host ""
Write-Host "インストール完了!" -ForegroundColor Green
Write-Host "システムトレイに github-notifier-ws が追加されました。"
Write-Host "初回起動時に GitHub PAT (notifications スコープ) の入力を求められます。"
Write-Host ""
