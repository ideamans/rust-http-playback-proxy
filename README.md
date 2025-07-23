# HTTP Playback Proxy

Rustで実装されたHTTPトラフィックの録画・再生プロキシサーバー。Webページの読み込み性能を測定・分析するためのツールです。

## 概要

このプログラムは、HTTPリクエスト/レスポンスを録画し、後で同じタイミングで再生することができるMITMプロキシです。PageSpeed最適化やパフォーマンステストに使用できます。

### 主な機能

- **録画モード**: HTTPプロキシとしてトラフィックを録画
- **再生モード**: 録画したトラフィックを同じタイミングで再生
- **HTTPS対応**: 自己署名証明書による HTTPS プロキシ（証明書エラーは無視）
- **レスポンス処理**: 圧縮解除、文字エンコーディング変換、Minify/Beautify
- **タイミング制御**: TTFB（Time To First Byte）と転送速度の再現

## 使用技術

- **言語**: Rust 2024 Edition
- **非同期ランタイム**: Tokio
- **HTTP**: Hyper, Reqwest
- **CLI**: Clap
- **シリアライゼーション**: Serde (JSON)
- **圧縮**: flate2, brotli
- **その他**: tracing, anyhow, tempfile

## インストール

```bash
git clone <repository-url>
cd rust-http-playback-proxy
cargo build --release
```

## 使用方法

### 録画モード

```bash
# 基本的な録画
./target/release/http-playback-proxy recording --port 8080 --inventory ./my-session

# エントリーURLを指定
./target/release/http-playback-proxy recording https://example.com --port 8080 --device desktop --inventory ./my-session
```

**パラメータ:**
- `entry_url`: 録画開始のエントリーURL（オプション）
- `--port`: プロキシサーバーのポート（デフォルト: 8080から自動検索）
- `--device`: デバイスタイプ（mobile/desktop、デフォルト: mobile）
- `--inventory`: インベントリディレクトリ（デフォルト: ./inventory）

**録画の流れ:**
1. プロキシサーバーが指定ポートで起動
2. ブラウザのプロキシ設定を `127.0.0.1:8080` に設定
3. Webサイトにアクセス
4. `Ctrl+C` で録画終了
5. `inventory.json` とコンテンツファイルが保存される

### 再生モード

```bash
./target/release/http-playback-proxy playback --port 8080 --inventory ./my-session
```

**パラメータ:**
- `--port`: プロキシサーバーのポート（デフォルト: 8080から自動検索）
- `--inventory`: 録画データのディレクトリ

**再生の流れ:**
1. `inventory.json` から録画データを読み込み
2. プロキシサーバーが指定ポートで起動
3. ブラウザで同じURLにアクセス
4. 録画時と同じタイミングでレスポンスが返される

## データ構造

### Inventory
```json
{
  "entryUrl": "https://example.com",
  "deviceType": "mobile",
  "resources": [...]
}
```

### Resource
```json
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
  "contentTypeMime": "text/css",
  "contentTypeCharset": "utf-8",
  "contentFilePath": "get/https/example.com/style.css",
  "minify": true
}
```

## テスト

### 単体テスト

```bash
# 全ての単体テストを実行
cargo test

# 特定のモジュールのテストを実行
cargo test recording
cargo test playback

# テスト詳細表示
cargo test -- --nocapture

# リリースモードでテスト
cargo test --release
```

### テストカバレッジ

```bash
# tarpaulinをインストール（初回のみ）
cargo install cargo-tarpaulin

# カバレッジレポート生成
cargo tarpaulin --out Html --output-dir coverage

# ブラウザでレポートを確認
open coverage/tarpaulin-report.html
```

### 結合テスト

結合テストは実際のHTTPサーバーとプロキシを起動して、エンドツーエンドのテストを行います。

```bash
# 結合テストを実行
cargo test --test integration_test

# 詳細ログ付きで実行
RUST_LOG=info cargo test --test integration_test -- --nocapture

# リリースモードで実行（推奨）
cargo test --test integration_test --release -- --nocapture
```

**結合テストの内容:**
1. **静的Webサーバー起動**: HTML, CSS, JavaScriptを提供
2. **録画プロキシ起動**: 指定ポートでHTTPプロキシを開始
3. **HTTPリクエスト送信**: プロキシ経由でコンテンツを取得
4. **ファイル確認**: `inventory.json` とコンテンツファイルの作成確認
5. **再生プロキシ起動**: 録画データから再生プロキシを開始
6. **再生確認**: 録画時と同じコンテンツが返されることを確認

### テストのトラブルシューティング

**ポート競合エラー:**
```bash
# 使用中のポートを確認
lsof -i :8080

# プロセスを終了
kill -9 <PID>
```

**バイナリビルドエラー:**
```bash
# クリーンビルド
cargo clean
cargo build --release
```

**テストタイムアウト:**
```bash
# タイムアウト時間を延長
cargo test -- --test-threads=1 --timeout=60
```

## 開発

### プロジェクト構造

```
src/
├── main.rs              # エントリーポイント
├── cli.rs              # CLI定義
├── types.rs            # データ型定義
├── traits.rs           # 依存性注入用トレイト
├── utils.rs            # ユーティリティ関数
├── recording/          # 録画モード
│   ├── mod.rs
│   ├── proxy.rs        # HTTPプロキシサーバー
│   └── processor.rs    # レスポンス処理
└── playback/           # 再生モード
    ├── mod.rs
    ├── proxy.rs        # HTTPプロキシサーバー
    └── transaction.rs  # トランザクション変換

tests/
└── integration_test.rs # 結合テスト
```

### コード品質

**Lint実行:**
```bash
cargo clippy
cargo clippy -- -D warnings  # 警告をエラーとして扱う
```

**フォーマット:**
```bash
cargo fmt
cargo fmt -- --check  # フォーマットチェックのみ
```

**型チェック:**
```bash
cargo check
```

### 依存性注入とテスト

テスタビリティ向上のため、以下のトレイトを使用した依存性注入を実装:

- `FileSystem`: ファイルシステム操作の抽象化
- `TimeProvider`: 時間取得の抽象化
- `HttpClient`: HTTP通信の抽象化（将来の拡張用）

```rust
// 本番環境
let processor = RequestProcessor::new(
    inventory_dir,
    Arc::new(RealFileSystem),
    Arc::new(RealTimeProvider::new())
);

// テスト環境
let processor = RequestProcessor::new(
    inventory_dir,
    Arc::new(MockFileSystem::new()),
    Arc::new(MockTimeProvider::new())
);
```

## パフォーマンス

- **メモリ使用量**: 通常のWebページ読み込みを想定し、メモリを潤沢に使用
- **並行処理**: Tokioによる非同期処理でリクエストを並行処理
- **ストリーミング**: 大きなレスポンスも効率的に処理

## トラブルシューティング

### よくある問題

**1. プロキシに接続できない**
- ポートが使用中でないか確認
- ファイアウォール設定を確認
- ブラウザのプロキシ設定を確認

**2. HTTPS接続エラー**
- 証明書エラーは無視される設計
- ブラウザで証明書警告が出た場合は「続行」を選択

**3. 録画ファイルが作成されない**
- ディスク容量を確認
- 書き込み権限を確認
- `Ctrl+C` で正常終了したか確認

**4. 再生時にコンテンツが異なる**
- `inventory.json` の内容を確認
- コンテンツファイルの存在を確認
- ログでエラーメッセージを確認

## ライセンス

[ライセンス情報を追加してください]

## 貢献

プルリクエストやイシューの報告を歓迎します。

## 更新履歴

- v0.1.0: 初期実装
  - 録画・再生機能
  - HTTPS対応
  - 依存性注入によるテスタビリティ向上
  - 結合テスト実装