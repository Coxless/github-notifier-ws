# github-notifier-ws

GitHub の通知をデスクトップのトースト通知として届ける、Windows 常駐型ユーティリティ。

一般的な通知クライアントと異なり、受信箱 UI を持たない。ユーザーが **YAML ファイルに書いたルール**で通知を濾し、本当に必要なものだけを静かにトーストとして提示することに特化する。

**「静かに、濾して、必要な分だけ」**

---

## 特徴

- **YAML 駆動のフィルタルール** — GUI の設定画面を持たず、YAML がすべての真実
- **4 軸フィルタ** — `reason` / `repository` / `subject_type` / `title_contains` の組み合わせ
- **first-match-wins** — ルールは上から順に評価され、最初にマッチしたルールが適用される
- **Windows ネイティブトースト** — プロトコル起動アクション付き。Action Center にも残る
- **ライブリロード** — 設定ファイルを保存するだけで即時反映。パースエラーは直前の設定で継続
- **軽量** — トレイアイコン一点の常設 UI。常時 UI ウィンドウを持たない

---

## インストール

### リリースバイナリ（推奨）

1. [Releases](../../releases) から最新の `github-notifier-ws_x.x.x_x64-setup.exe` をダウンロード
2. インストーラを実行

> Action Center にトーストを残すには、インストーラ経由で AUMID + CLSID が登録される必要があります。インストーラを使わずに `.exe` を直接起動した場合、トーストはポップアップされますが Action Center に残りません。

### PowerShell スクリプト（コンテナイメージからインストール）

```powershell
irm https://raw.githubusercontent.com/coxless/github-notifier-ws/main/install.ps1 | iex
```

---

## セットアップ

初回起動時にトークン入力ダイアログが表示されます。

1. GitHub で `notifications` スコープの **Personal Access Token (classic)** を発行
   - プライベートリポジトリの通知も取得したい場合は `repo` スコープも追加
2. ダイアログにトークンを入力して「保存して開始」

トークンは **Windows Credential Manager** に保存されます。YAML ファイルには書きません。

---

## 設定ファイル

パス: `%APPDATA%\github-notifier-ws\config.yaml`

トレイメニューの「設定ファイルを開く」からエディタで直接開けます。

```yaml
poll_interval: 60          # ポーリング間隔の下限（秒）。X-Poll-Interval が優先
bundle_threshold: 3        # 1回のポーリングでこの件数以上なら束ねて1枚に
allow_destructive: false   # mark_read / mark_done を実際に発火させるか
default: ignore            # どのルールにもマッチしない通知の扱い

rules:
  - name: direct
    match:
      reason: [mention, review_requested, assign]
    action: notify

  - name: active-repo PRs
    match:
      repository: octocat/api-server
      subject_type: PullRequest
    action: notify

  - name: ci-noise
    match:
      reason: ci_activity
    action: mark_read       # allow_destructive: true のときだけ実発火

  - name: urgent-by-title
    match:
      subject_type: Issue
      title_contains: "[urgent]"
    action: notify
```

### フィルタ軸

| キー | 型 | 説明 |
|------|----|------|
| `reason` | `string[]` | 通知のきっかけ（any-of マッチ） |
| `repository` | `string` | `owner/repo` の完全一致 |
| `subject_type` | `string` | `Issue` / `PullRequest` / `Commit` / `Release` / `Discussion` |
| `title_contains` | `string` | タイトルの部分一致（大文字小文字を区別する） |

`reason` に指定できる主な値: `mention` / `review_requested` / `assign` / `ci_activity` / `team_mention` / `state_change` / `security_alert` / `subscribed`

空の `match` はすべての通知にマッチします（末尾の catch-all ルールとして使用可）。

### アクション

| アクション | 動作 | 破壊的 |
|-----------|------|--------|
| `notify` | トースト表示 | — |
| `ignore` | 何もしない。GitHub 側は未読のまま | — |
| `mark_read` | GitHub 側で既読化 | ✓ opt-in |
| `mark_done` | 既読化 + 購読解除 | ✓ opt-in |

`mark_read` / `mark_done` は `allow_destructive: true` がない限り `notify` にフォールバックします。

---

## トレイアイコン

状態を**色**で表現します。

| 状態 | アイコン | 意味 |
|------|---------|------|
| idle | グレー | 未読なし |
| unread | グリーン | 未読通知あり |
| paused | ダークグレー | 一時停止中 |
| error | レッド | 認証切れ・設定破損など |

**左クリック** — `github.com/notifications` をブラウザで開く  
**右クリック** — コンテキストメニュー

---

## コンテキストメニュー

```
  未読なし
  30秒前に同期  ·  API残 4,832
──────────────────
今すぐ確認
通知を一時停止  ▶  30分 / 1時間 / 明日まで止める / 一時停止を解除
──────────────────
設定ファイルを開く
設定を再読み込み
──────────────────
GitHubで開く
──────────────────
終了
```

- **今すぐ確認** — ポーリング間隔を無視して即時チェック
- **通知を一時停止** — スヌーズ中はアイコンがグレーに。「明日まで止める」は翌日 9:00 まで
- **設定を再読み込み** — 手動でリロード（通常はファイル保存で自動リロードされる）

---

## トースト

### 単発トースト

```
┌─────────────────────────────────────┐
│ owner/repo                          │
│ あなたがメンションされました · Issue │
│ Fix the critical bug in production  │
│  [開く]  [既読にする]  [このリポジトリをミュート] │
└─────────────────────────────────────┘
```

- **開く** — スレッドをブラウザで開く
- **既読にする** — GitHub 側で既読化（`allow_destructive` の設定に関わらず実行）
- **このリポジトリをミュート** — 設定ファイルに `ignore` ルールを自動追記

### 束ねトースト（`bundle_threshold` 件以上）

```
┌─────────────────────────────────────┐
│ 5 件の新しい通知                    │
│ メンション 2・レビュー依頼 1・その他 2 │
│  [受信箱を開く]  [すべて既読にする]  │
└─────────────────────────────────────┘
```

> 「すべて既読にする」は `allow_destructive: true` がないと動作しません（受信箱を開くにフォールバック）。

---

## ライブリロードとエラー処理

- 設定ファイルを保存すると自動でリロードされます
- YAML のパースに失敗した場合、**直前の有効な設定で動作を継続**します（全停止しない）
- エラー内容と行番号はトーストで通知されます

---

## ビルド

### 前提条件

- [Rust](https://rustup.rs/) 1.77 以上
- [Node.js](https://nodejs.org/) 18 以上（Tauri CLI のため）
- Windows 10 SDK（Windows ターゲットでビルドする場合）

```bash
# アイコン生成
python scripts/gen-icons.py

# 開発ビルド（Windows でのみ動作確認済み）
cd src-tauri
cargo build

# リリースビルド（インストーラ付き）
cargo tauri build
```

---

## アーキテクチャ

```
poll /notifications ─► 差分検出 ─► ルール判定 ─► アクション
   ▲ (If-Modified-Since,                          │
   │  X-Poll-Interval を下限に)         notify ────┼─► トースト（プロトコルアクション）
   └──────── sleep/wake ◄─────────────            │
        ↑ 「今すぐ確認」で即時ウェイク  mark_read ─┤─► PATCH thread
                                       mark_done ─┘─► DELETE subscription
              トーストクリック
                    │
        github-notifier-ws://thread/<id>?do=…
                    │
        単一インスタンスが URL を転送 ─► アクション処理 ─► ブラウザ起動 / 既読化
```

### モジュール構成

| モジュール | 責務 |
|-----------|------|
| `main` | プラグイン配線、トレイ、ウォッチャ、ポーリングループ |
| `config` | YAML スキーマ、ロード、ライブリロード、last-good フォールバック、ミュートルール追記 |
| `github` | API クライアント（poll / mark_read / mark_done）、トークン取得 |
| `rules` | 4 軸での first-match-wins 判定 |
| `notify` | プロトコルアクション付きトーストの構築 |
| `actions` | `github-notifier-ws://` ディープリンクの解釈と分岐 |
| `tray` | トレイアイコン状態・コンテキストメニュー・動的ステータス表示 |

---

## セキュリティ

- GitHub トークンは YAML に平文で保存せず、Windows Credential Manager に格納
- 通信は GitHub API（HTTPS）に限定。第三者サーバーへの送信なし
- 破壊的アクション（既読化・購読解除）は既定で無効

---

## ライセンス

MIT
