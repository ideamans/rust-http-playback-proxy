# HTTP Playback Proxy

日本語 | [English](./README.md)

[![CI](https://github.com/pagespeed-quest/http-playback-proxy/actions/workflows/ci.yml/badge.svg)](https://github.com/pagespeed-quest/http-playback-proxy/actions/workflows/ci.yml)
[![Release](https://github.com/pagespeed-quest/http-playback-proxy/actions/workflows/release.yml/badge.svg)](https://github.com/pagespeed-quest/http-playback-proxy/actions/workflows/release.yml)

正確なタイミング制御でWebトラフィックを録画・再生するMITM HTTP/HTTPSプロキシ。PageSpeed最適化、パフォーマンステスト、Webパフォーマンス自動分析のために設計されています。

## 特徴

- **録画モード**: MITM ProxyとしてHTTP/HTTPSトラフィックをタイミングメタデータ付きでキャプチャ
- **再生モード**: 録画したトラフィックをTTFBと転送時間を正確にシミュレートして再生
- **コンテンツ処理**: Minify化されたHTML/CSS/JSを自動的にBeautify化して編集可能に
- **HTTPS対応**: 自己署名証明書を使用した透過的なHTTPSプロキシ
- **タイミング精度**: TTFBと転送時間を±10%の精度で再現
- **マルチプラットフォーム**: macOS (ARM64/x86_64)、Linux (x86_64/ARM64)、Windows (x86_64)対応
- **言語ラッパー**: GoとTypeScript/Node.js向けのバインディングで簡単統合

## クイックスタート

### ビルド済みバイナリの使用

[GitHub Releases](https://github.com/pagespeed-quest/http-playback-proxy/releases)からダウンロード：

```bash
# macOS ARM64
curl -L https://github.com/pagespeed-quest/http-playback-proxy/releases/latest/download/http-playback-proxy-darwin-arm64.tar.gz | tar xz

# Linux x86_64
curl -L https://github.com/pagespeed-quest/http-playback-proxy/releases/latest/download/http-playback-proxy-linux-amd64.tar.gz | tar xz

# Windows x86_64
# http-playback-proxy-windows-amd64.zipをダウンロードして展開
```

### コマンドライン使用方法

#### 録画モード

**基本的な録画 (ポート8080から自動検索):**
```bash
./http-playback-proxy recording https://example.com
```

**全オプション:**
```bash
./http-playback-proxy recording https://example.com \
  --port 8080 \              # プロキシポート (デフォルト: 8080、使用中なら自動検索)
  --device mobile \           # デバイスタイプ: mobile または desktop (デフォルト: mobile)
  --inventory ./my-session    # 出力ディレクトリ (デフォルト: ./inventory)
```

**録画の流れ:**
1. プロキシ起動: `./http-playback-proxy recording https://example.com`
2. ブラウザのプロキシを`127.0.0.1:8080` (または表示されたポート)に設定
3. ブラウザでWebサイトを訪問
4. `Ctrl+C`で停止して録画を保存
5. `./inventory/inventory.json`と`./inventory/contents/`を確認

**手動ブラウジング (エントリURLなし):**
```bash
# プロキシを起動して手動でブラウジング
./http-playback-proxy recording --port 8080
```

#### 再生モード

**基本的な再生:**
```bash
./http-playback-proxy playback --inventory ./my-session
```

**全オプション:**
```bash
./http-playback-proxy playback \
  --port 8080 \               # プロキシポート (デフォルト: 8080、使用中なら自動検索)
  --inventory ./my-session    # 録画データディレクトリ (デフォルト: ./inventory)
```

**再生の流れ:**
1. プロキシ起動: `./http-playback-proxy playback --inventory ./my-session`
2. ブラウザのプロキシを`127.0.0.1:8080` (または表示されたポート)に設定
3. 同じWebサイトを訪問 - 録画時のタイミング(±10%)でレスポンスが返される
4. `Ctrl+C`で停止

#### ブラウザプロキシ設定

**Chrome/Chromium:**
```bash
# macOS/Linux
google-chrome --proxy-server="127.0.0.1:8080"

# Windows
chrome.exe --proxy-server="127.0.0.1:8080"
```

**Firefox:**
設定 → ネットワーク設定 → 手動でプロキシを設定:
- HTTPプロキシ: `127.0.0.1`、ポート: `8080`
- 「このプロキシをHTTPSにも使用する」にチェック

**システム全体 (macOS):**
```bash
# プロキシを設定
networksetup -setwebproxy Wi-Fi 127.0.0.1 8080
networksetup -setsecurewebproxy Wi-Fi 127.0.0.1 8080

# プロキシを解除
networksetup -setwebproxystate Wi-Fi off
networksetup -setsecurewebproxystate Wi-Fi off
```

## インストール

### ソースからビルド (Rust)

```bash
git clone https://github.com/pagespeed-quest/http-playback-proxy.git
cd http-playback-proxy
cargo build --release
```

バイナリの場所: `target/release/http-playback-proxy`

### Goモジュール

```bash
go get github.com/pagespeed-quest/http-playback-proxy/golang
```

**録画の例:**
```go
package main

import (
    "fmt"
    "time"
    proxy "github.com/pagespeed-quest/http-playback-proxy/golang"
)

func main() {
    // 録画プロキシを起動
    p, err := proxy.StartRecording(proxy.RecordingOptions{
        EntryURL:     "https://example.com",
        Port:         8080,
        DeviceType:   proxy.DeviceTypeMobile,
        InventoryDir: "./inventory",
    })
    if err != nil {
        panic(err)
    }

    fmt.Printf("Recording proxy started on port %d\n", p.Port)

    // プロキシ経由でHTTPリクエストを実行または待機
    time.Sleep(30 * time.Second)

    // 停止して録画を保存
    if err := p.Stop(); err != nil {
        panic(err)
    }

    // Inventoryを読み込んで分析
    inventory, err := p.GetInventory()
    if err != nil {
        panic(err)
    }

    fmt.Printf("Recorded %d resources\n", len(inventory.Resources))
}
```

**再生の例:**
```go
// 再生プロキシを起動
p, err := proxy.StartPlayback(proxy.PlaybackOptions{
    Port:         8080,
    InventoryDir: "./inventory",
})
if err != nil {
    panic(err)
}

// リクエストを待機
time.Sleep(30 * time.Second)

// 再生を停止
p.Stop()
```

**Inventoryの操作:**
```go
// Inventoryを読み込み
inventory, err := proxy.LoadInventory("./inventory/inventory.json")
if err != nil {
    panic(err)
}

// リソースを反復処理
for i, resource := range inventory.Resources {
    fmt.Printf("%d: %s %s (TTFB: %dms)\n",
        i, resource.Method, resource.URL, resource.TtfbMs)

    // コンテンツファイルのパスを取得
    if resource.ContentFilePath != nil {
        contentPath := proxy.GetResourceContentPath("./inventory", &resource)
        // コンテンツファイルを読み込み...
    }
}
```

完全なAPI仕様は [golang/README.md](golang/README.md) を参照してください。

### TypeScript/Node.jsパッケージ

```bash
npm install http-playback-proxy
```

**録画の例:**
```typescript
import { startRecording } from 'http-playback-proxy';

async function record() {
  // 録画プロキシを起動
  const proxy = await startRecording({
    entryUrl: 'https://example.com',
    port: 8080,
    deviceType: 'mobile',
    inventoryDir: './inventory',
  });

  console.log(`Recording proxy started on port ${proxy.port}`);

  // プロキシ経由でHTTPリクエストを実行または待機
  await new Promise(resolve => setTimeout(resolve, 30000));

  // 停止して録画を保存
  await proxy.stop();

  // Inventoryを読み込んで分析
  const inventory = await proxy.getInventory();
  console.log(`Recorded ${inventory.resources.length} resources`);
}

record().catch(console.error);
```

**再生の例:**
```typescript
import { startPlayback } from 'http-playback-proxy';

async function playback() {
  // 再生プロキシを起動
  const proxy = await startPlayback({
    port: 8080,
    inventoryDir: './inventory',
  });

  console.log(`Playback proxy started on port ${proxy.port}`);

  // リクエストを待機
  await new Promise(resolve => setTimeout(resolve, 30000));

  // 再生を停止
  await proxy.stop();
}

playback().catch(console.error);
```

**Inventoryの操作:**
```typescript
import { loadInventory, getResourceContentPath } from 'http-playback-proxy';

// Inventoryを読み込み
const inventory = await loadInventory('./inventory/inventory.json');

// リソースを反復処理
for (const [i, resource] of inventory.resources.entries()) {
  console.log(`${i}: ${resource.method} ${resource.url} (TTFB: ${resource.ttfbMs}ms)`);

  // コンテンツファイルのパスを取得
  if (resource.contentFilePath) {
    const contentPath = getResourceContentPath('./inventory', resource);
    // コンテンツファイルを読み込み...
  }
}
```

完全なAPI仕様は [typescript/README.md](typescript/README.md) を参照してください。

## アーキテクチャ

### コア実装 (Rust)

- **ランタイム**: Tokio非同期ランタイムで並行リクエスト処理
- **HTTPスタック**: Hyper 1.0, Hyper-util, Tower/Tower-http
- **MITM Proxy**: Hudsucker 0.24 + rcgen-caで自己署名証明書
- **コンテンツ処理**: 自動Beautify化 (prettyish-html, prettify-js)
- **圧縮**: gzip, deflate, brotli対応 (flate2, brotliクレート)
- **エンコーディング**: 文字セット検出とUTF-8変換 (encoding_rs)

### データ構造

録画データの保存形式：
- `inventory.json`: 全リソースのメタデータ (URL、タイミング、ヘッダー)
- `contents/`: method/protocol/pathで整理されたコンテンツファイル

**Inventory構造:**
```json
{
  "entryUrl": "https://example.com",
  "deviceType": "mobile",
  "resources": [
    {
      "method": "GET",
      "url": "https://example.com/style.css",
      "ttfbMs": 150,
      "mbps": 2.5,
      "statusCode": 200,
      "rawHeaders": {
        "content-type": "text/css; charset=utf-8"
      },
      "contentEncoding": "gzip",
      "contentFilePath": "get/https/example.com/style.css",
      "minify": true
    }
  ]
}
```

### 言語ラッパー

**Go**: プロセスマネージャラッパー + Inventoryヘルパー
- バイナリのライフサイクル管理 (start/stop)
- 型安全なInventory読み書き
- Goroutineベースのリクエスト処理

**TypeScript/Node.js**: Node.jsエコシステム向け同様のラッパー
- PromiseベースのAPI
- npmパッケージ配布
- 完全なTypeScript型定義

## テストエコシステム

### ユニットテスト (Rust)

```bash
cargo test                    # 全ユニットテスト
cargo test recording          # Recordingモジュールのテスト
cargo test playback           # Playbackモジュールのテスト
cargo test -- --nocapture     # 詳細出力付き
```

### 統合テスト (Rust)

`tests/integration_test.rs`に配置。Rustの完全なE2Eテスト：

```bash
cargo test --test integration_test --release -- --nocapture
```

テスト内容: 録画 → Inventory保存 → 再生 → コンテンツ検証

### E2Eテスト (コア機能)

`e2e/`に配置。コアバイナリの機能テスト：

```bash
cd e2e
make test-all                 # 全E2Eテスト
make test-performance         # パフォーマンス/ストレステスト
make test-minimum             # タイミング精度テスト (6シナリオ)
make test-content             # コンテンツBeautify化テスト
```

**Minimum Timingテスト** (`e2e/minimum/`):
- ファイルサイズとレイテンシが異なる6シナリオをテスト
- TTFBと転送時間の±10%精度を検証
- 実行時間: 約60-90秒

**Content Beautificationテスト** (`e2e/content/`):
- 録画時にMinify化されたHTML/CSS/JSがBeautify化されることを検証
- inventoryの`minify: true`フラグをチェック
- PageSpeed最適化のためにコンテンツが編集可能であることを保証
- 実行時間: 約10秒

詳細は [e2e/README.md](e2e/README.md) を参照してください。

### 受け入れテスト (言語ラッパー)

`acceptance/`に配置。GoとTypeScriptラッパーのテスト：

```bash
cd acceptance
make test-all                 # Go・TypeScriptラッパーの両方をテスト
make test-golang              # Goラッパーのみ
make test-typescript          # TypeScriptラッパーのみ
```

**Go受け入れテスト** (`acceptance/golang/`):
- Go API (StartRecording, StartPlayback, Stop)の検証
- Goモジュール内のバイナリ配布をテスト
- Inventoryの読み書きを検証

**TypeScript受け入れテスト** (`acceptance/typescript/`):
- TypeScript APIの検証
- npmパッケージ内のバイナリ配布をテスト
- Promiseベースのワークフローを検証

詳細は [acceptance/README.md](acceptance/README.md) を参照してください。

## CI/CDワークフロー

コミット前チェック (ローカルで実行):
```bash
./check-ci.sh                 # CIと全く同じチェックをローカルで実行
```

このスクリプトは以下を実行:
1. `cargo fmt --all -- --check` - フォーマット検証
2. `cargo clippy --all-targets --all-features -- -D warnings` - 厳格なLint
3. `cargo test` - 全テスト (ユニット + 統合)

### リリースワークフロー

マルチプラットフォーム自動リリース：

```
1. タグ作成:          git tag v0.0.0 && git push origin v0.0.0
2. GitHub Actions:    5プラットフォーム向けバイナリをビルド (release.yml)
3. リリース作成:      GitHub Releasesに公開
4. 自動トリガー:      update-binaries.ymlワークフロー
5. PR作成:            バイナリ → golang/bin/ と typescript/bin/
6. 受け入れテスト:    全プラットフォームでテスト (acceptance-test.yml)
7. PRマージ:          テスト通過後
8. Goタグ:            git tag golang/v0.0.0 && git push
9. npm公開:           cd typescript && npm publish
```

対応プラットフォーム:
- darwin-arm64 (macOS Apple Silicon)
- darwin-amd64 (macOS Intel)
- linux-amd64 (Linux x86_64)
- linux-arm64 (Linux ARM64)
- windows-amd64 (Windows x86_64)

## 開発

### プロジェクト構造

```
.
├── src/                     # Rustコア実装
│   ├── recording/           # 録画モード (MITMプロキシ、レスポンス処理)
│   ├── playback/            # 再生モード (タイミング制御、トランザクションマッチング)
│   └── ...
├── tests/                   # Rust統合テスト
├── e2e/                     # コアE2Eテスト (パフォーマンス、タイミング、コンテンツ)
├── acceptance/              # 言語ラッパー受け入れテスト
├── golang/                  # Go言語ラッパー + テスト
├── typescript/              # TypeScript/Node.jsラッパー + テスト
└── .github/workflows/       # CI/CDワークフロー
```

### コード品質

**コミット前チェック (推奨):**
```bash
./check-ci.sh                # CIと全く同じチェックをローカルで実行
```

このスクリプトは以下を実行:
1. `cargo fmt --all -- --check` - フォーマット検証
2. `cargo clippy --all-targets --all-features -- -D warnings` - 厳格なLint
3. `cargo test` - 全テスト (ユニット + 統合)

**個別コマンド:**
```bash
cargo fmt                    # コード自動フォーマット
cargo clippy                 # Lintチェック
cargo test                   # 全テスト実行
cargo build --release        # リリースビルド
```

### 主要な実装機能

**録画:**
- Hudsuckerによる自己署名証明書を使用したMITMプロキシ
- リクエスト/レスポンス相関のための接続単位のFIFOキュー
- 自動コンテンツBeautify化 (Minify化されたHTML/CSS/JS)
- 複数値ヘッダー対応 (例: Set-Cookie)
- UTF-8変換と文字セット検出

**再生:**
- 正確なタイミング制御 (TTFBと転送時間の±10%精度)
- ターゲット時間でのチャンクベースのレスポンスストリーミング
- method + host + path + queryによるトランザクションマッチング
- 自動再Minify化と再エンコード

**テスタビリティ:**
- トレイトベースの依存性注入 (FileSystem, TimeProvider)
- ユニットテスト用のモック実装
- 包括的なテストカバレッジ (ユニット、統合、E2E、受け入れ)

## トラブルシューティング

**プロキシ接続の問題:**
- ポートの空き状況を確認: `lsof -i :8080`
- ファイアウォール設定を確認
- ブラウザのプロキシ設定を確認

**HTTPS証明書エラー:**
- ブラウザ: 「詳細設定」→「続行」をクリック (証明書は自己署名)
- システム信頼: 必要に応じて証明書をシステムの信頼ストアに追加

**バイナリが見つからない (テスト):**
```bash
cargo build --release
ls -la target/release/http-playback-proxy
```

**ポートの競合:**
- テストは自動割り当てポート (port 0)を使用
- スタックしたプロセスを終了: `lsof -i :8080 && kill -9 <PID>`

**タイミング精度の問題:**
- システム負荷を確認 (高CPU/ディスク使用率はタイミングに影響)
- ネットワークの安定性を確認
- 期待される許容範囲については、Minimum Timingテストを参照

## コントリビューション

コントリビューションを歓迎します！以下をお願いします：
1. コミット前に`./check-ci.sh`を実行
2. 新機能にはテストを追加
3. ドキュメントを更新
4. 既存のコードスタイルに従う

## ライセンス

[ライセンス情報を追加してください]

## 関連ドキュメント

- [CLAUDE.md](CLAUDE.md) - AI支援開発のためのガイダンス
- [E2EテストREADME](e2e/README.md) - コアE2Eテスト
- [受け入れテストREADME](acceptance/README.md) - 言語ラッパーテスト
- [GoラッパーREADME](golang/README.md) - Go API仕様
- [TypeScriptラッパーREADME](typescript/README.md) - TypeScript API仕様
