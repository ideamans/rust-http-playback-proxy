# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 概要

HTTP Playback Proxyは、HTTPトラフィックを録画・再生し、正確なタイミング制御を行うRust製MITMプロキシです。PageSpeed最適化やパフォーマンステストに使用します。

## 開発コマンド

```bash
# ビルド
cargo build --release

# テスト実行（単体テスト）
cargo test

# 特定モジュールのテスト
cargo test recording
cargo test playback

# 結合テスト（事前にバイナリビルドが必要）
cargo test --test integration_test --release -- --nocapture

# Lint とフォーマット
cargo clippy
cargo fmt

# テストカバレッジ
cargo install cargo-tarpaulin  # 初回のみ
cargo tarpaulin --out Html --output-dir coverage
```

## CLIインターフェース

```bash
# 録画モード
./target/release/http-playback-proxy recording [entry_url] \
  --port <port> \
  --device <desktop|mobile> \
  --inventory <inventory_dir>

# 再生モード
./target/release/http-playback-proxy playback \
  --port <port> \
  --inventory <inventory_dir>

# デフォルト値:
# - ポート: 8080から自動検索
# - デバイス: mobile
# - インベントリ: ./inventory
```

## アーキテクチャ

### モジュール構造

```
src/
├── main.rs           # エントリポイント
├── cli.rs            # Clapコマンド定義
├── types.rs          # コアデータ型（Resource, Inventory, Transaction, BodyChunk）
├── traits.rs         # 依存性注入用トレイト（FileSystem, TimeProvider, HttpClient）
├── utils.rs          # ユーティリティ（ポート検索、エンコード/デコード、Minify）
├── recording/
│   ├── mod.rs        # 録画モードエントリポイント
│   ├── proxy.rs      # 録画用HTTPプロキシサーバー
│   └── processor.rs  # レスポンス処理（圧縮、文字セット、Minify）
└── playback/
    ├── mod.rs        # 再生モードエントリポイント
    ├── proxy.rs      # 再生用HTTPプロキシサーバー
    └── transaction.rs # ResourceからTransactionへの変換
```

### 主要な設計パターン

**トレイトベースの依存性注入**

フレームワークなしでトレイトベースDIを実装し、テスタビリティを確保：

- `FileSystem` - ファイルI/O抽象化
- `TimeProvider` - 時間計測抽象化
- `HttpClient` - HTTPリクエスト抽象化（将来の拡張用）
- モック実装は`traits::mocks`モジュール

**録画フロー**
1. HTTPプロキシサーバー起動（指定ポートでリッスン）
2. リクエストをキャプチャし、上流サーバーへ転送
3. レスポンスをクライアントへストリーミングしながらメモリに記録：
   - TTFB（Time To First Byte）
   - ヘッダ、ステータスコード、ボディ（圧縮済み）
   - 転送時間（Mbps計算用）
4. Ctrl+C（SIGINT）でインベントリ保存とリソース処理：
   - レスポンスボディを解凍
   - テキストリソースをUTF-8に変換
   - Beautifyしてminify検出（行数が2倍以上 = minified）
   - `inventory_dir/contents/<method>/<protocol>/<path>`に保存

**再生フロー**
1. `inventory.json`を読み込み
2. ResourceをTransactionに変換：
   - `minify: true`の場合は再度minify
   - 再エンコード（gzip/br/etc）
   - チャンクに分割し、タイムスタンプ設定
3. HTTPプロキシサーバー起動
4. リクエストに対応するTransactionを検索
5. タイミング制御で再生：
   - TTFBまで待機
   - `targetTime`に従ってチャンク送信（元の転送速度をシミュレート）

### データ型の互換性

**重要**: `Resource`と`Inventory`型は`reference/types.ts`のTypeScript定義と強い互換性を維持する必要があります。その他の内部型（Transaction, BodyChunk）はパフォーマンス最適化可能です。

### コンテンツファイルパス生成

URLからファイルパスへの変換規則：

- ベース: `inventory_dir/contents/<method>/<protocol>/<path>`
- インデックス処理: `/` → `/index.html`
- クエリパラメータ:
  - ≤32文字: `resource~param=value.html`
  - >32文字: `resource~param=first32chars.~<sha1(rest)>.html`

### パフォーマンス考慮事項

- **メモリファースト設計**: 通常のWebページサイズを想定し、速度のためメモリを潤沢に使用
- **Async/await**: Tokioランタイムで並行リクエスト処理
- **ストリーミング**: 大きなレスポンスも効率的に処理
- **事前処理**: 再生モードは全ResourceをTransactionに事前変換して高速提供

## テスト戦略

### 単体テスト

- `<module>/tests.rs`ファイルに配置
- `traits::mocks`のモック実装を使用
- コンポーネントの独立した動作に焦点

### 結合テスト

`tests/integration_test.rs`に実装。完全なエンドツーエンドテスト：

1. バイナリをビルド（`cargo build`または`cargo build --release`）
2. 埋め込み静的HTTPサーバー起動（HTML/CSS/JS提供）
3. 一時インベントリディレクトリで録画プロキシ起動
4. `reqwest`クライアントでプロキシ経由HTTPリクエスト
5. 録画プロキシにSIGINT送信（グレースフルシャットダウン）
6. `inventory.json`と`contents/`ファイルの確認
7. 録画データで再生プロキシ起動
8. 再生レスポンスが録画と一致することを確認（空白正規化済み）

**注意**: 結合テストは動的ポート検索で競合回避。Unix系システムでは`libc::kill()`でSIGINTによるグレースフルシャットダウンを実現。

## 技術スタック

- **Rust Edition**: 2024
- **非同期ランタイム**: Tokio 1.0（全機能有効）
- **HTTPスタック**: Hyper 1.0, Hyper-util, Http-body-util, Tower/Tower-http
- **シリアライゼーション**: Serde + serde_json
- **圧縮**: flate2（gzip/deflate）, brotli
- **エンコーディング**: encoding_rs（文字セット変換）
- **Minify**: minifierクレート（HTML/CSS/JS）
- **CLI**: Clap 4.5（derive機能）
- **テスト**: tempfile, tokio-test, reqwest（結合テスト用）

## よく使うパターン

### 新しいテストの追加

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::mocks::{MockFileSystem, MockTimeProvider};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_something() {
        let fs = Arc::new(MockFileSystem::new());
        let time = Arc::new(MockTimeProvider::new(0));

        // テスト実装
    }
}
```

### Inventoryの操作

```rust
use crate::types::{Inventory, Resource};

let mut inventory = Inventory::new();
inventory.entry_url = Some("https://example.com".to_string());
inventory.device_type = Some(DeviceType::Desktop);

let resource = Resource::new("GET".to_string(), "https://example.com".to_string());
inventory.resources.push(resource);
```

## トラブルシューティング

**結合テストが"port already in use"で失敗**
- 動的ポート検索を使用しますが、手動クリーンアップが必要な場合：
  ```bash
  lsof -i :8080
  kill -9 <PID>
  ```

**結合テストでSIGINTが機能しない**
- Unix専用機能（`libc::kill`使用）
- Windowsでは強制終了にフォールバック

**テスト時にバイナリが見つからない**
- 結合テスト前に`cargo build`または`cargo build --release`を実行
- テストはdebugとreleaseの両方のバイナリ場所をチェック

## 実装の重要ポイント

### 録画モード処理詳細

- 最初のリクエスト受信を起点（0秒）とする
- レスポンスボディは圧縮されている場合はそのまま記録
- レスポンスボディの長さと、応答受信開始から完了までの時間を記録
- Mbps計算式: `(レスポンスボディの長さ / 応答受信開始から完了までの秒) / (1024 * 1024)`

### テキストリソースの処理

主要なテキストリソース（HTML、CSS、JavaScript）は特別処理：

1. UTF-8に変換：
   - ヘッダのCharsetまたはリソース中のCharset指定を参照
   - UTF-8変換後、ヘッダのCharsetをUTF-8にし、コンテンツ中のCharset指定は削除

2. Beautify処理：
   - Beautifyして改行数が2倍以上になったら、そのリソースはminifyされていたと判定
   - リソースの`minify: true`に設定

### 再生モードの事前処理

- Inventoryを読み込み後、すべてのリソースをTransactionに変換
- Transactionには以下を含む：
  - HTTPレスポンスを返すための事前処理済み情報
  - `minify: true`の場合は再度Minify
  - エンコーディングして、チャンクに分割
  - チャンクごとの送信開始目標時間（TTFBを含む目標送信オフセット時間）

### HTTPS対応

- MITM プロキシとして動作
- すべて自己署名証明書を使用
- HTTPSエラーはすべて無視する設計
