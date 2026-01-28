# Ghost Server Plan

## 背景と動機

### 現状の問題点

Playbackモードは以下の問題を抱えており、安定性の改善が困難な状態にある：

1. **Hudsuckerプロキシの複雑性**
   - MITMプロキシとしてリクエスト処理とレスポンス生成を同時に担当
   - グレースフルシャットダウンのメカニズムがない（`proxy_task.abort()`でハードキル）
   - HTTPプロトコル処理の責任が分散している

2. **タイミング制御の複雑性**
   - 各リクエストハンドラ内でTTFB待機とチャンク送信タイミングを制御
   - 同時リクエスト間でタイミングの同期が取れない
   - `tokio::select!`とストリーミングの組み合わせが複雑

3. **責任の混在**
   - リクエストマッチング、コンテンツ処理、タイミング制御、レスポンス生成が単一モジュールに集中
   - テストが困難で、問題の切り分けが難しい

4. **Recordingとの非対称性**
   - Recordingは「プロキシとして転送するだけ」のシンプルな動作
   - Playbackは「HTTPサーバーのふりをする」複雑な動作
   - 安定性に差が出るのは自然な結果

## 新アーキテクチャ: Ghost Server

### 設計思想

**「プロキシはプロキシらしく、サーバーはサーバーらしく」**

- Playback Proxyはリクエストの交通整理に専念（シンプルなリバースプロキシ）
- Ghost Serverがタイミング制御付きレスポンスを返す本質的な処理を担当
- 各コンポーネントが単一責任を持つことで安定性向上

### アーキテクチャ概要

```
                    ┌─────────────────────────────────────────────┐
                    │              Ghost Server                    │
                    │  (バーチャルホストWebサーバー)               │
                    │                                              │
                    │  - Inventoryからトランザクション構築         │
                    │  - タイミング制御付きレスポンス生成          │
                    │  - Hostヘッダによるリソースマッチング        │
                    │  - ポート: 内部ポート (例: 18081)            │
                    └─────────────────────────────────────────────┘
                                        ▲
                                        │ HTTP リクエスト
                                        │ (Host: example.com)
                                        │
┌─────────────┐     ┌─────────────────────────────────────────────┐
│   Browser   │────▶│           Playback Proxy                     │
│  (Client)   │◀────│  (シンプルなリバースプロキシ)                │
│             │     │                                              │
│ Proxy設定:  │     │  1. リクエスト受信                           │
│ localhost:  │     │  2. Hostヘッダを保持したままGhost Serverに転送│
│ 18080       │     │  3. レスポンスをそのまま返却                 │
│             │     │  ポート: 18080                               │
└─────────────┘     └─────────────────────────────────────────────┘
```

### コンポーネント詳細

#### 1. Ghost Server

**役割**: 録画されたHTTPトラフィックを正確なタイミングで再生するWebサーバー

**責任**:
- Inventoryの読み込みとTransactionへの変換
- Hostヘッダに基づくリクエストマッチング
- TTFBとチャンク送信のタイミング制御
- HTTPレスポンスの生成

**特徴**:
- 通常のHTTPサーバーとして実装（MITMプロキシではない）
- バーチャルホスト対応（複数ドメインを単一サーバーで処理）
- Hyper/Axumなど安定したHTTPサーバーフレームワークを使用可能
- グレースフルシャットダウンが容易

**エンドポイント設計**:
```
GET /any/path HTTP/1.1
Host: example.com

→ Hostヘッダ + パス + メソッドでトランザクションをマッチング
→ タイミング制御付きでレスポンスを返却
```

#### 2. Playback Proxy

**役割**: クライアントからのリクエストをGhost Serverに転送するシンプルなプロキシ

**責任**:
- HTTPプロキシとしてリクエストを受信
- リクエストをGhost Serverに転送（Hostヘッダを保持）
- レスポンスをそのままクライアントに返却

**特徴**:
- 最小限のロジック（ほぼパススルー）
- HTTPSリクエストの場合もMITMしてGhost Serverに転送
- タイミング制御は一切行わない（Ghost Serverに委譲）

**動作フロー**:
```
1. クライアント: GET https://example.com/page.html
2. Playback Proxy:
   - HTTPS接続をMITMで処理
   - リクエストをHTTPに変換してGhost Serverに転送:
     GET /page.html HTTP/1.1
     Host: example.com
3. Ghost Server: タイミング制御付きでレスポンス返却
4. Playback Proxy: レスポンスをそのままクライアントに返却
```

### HTTPSの処理

Ghost ServerはHTTPのみで動作し、HTTPSはPlayback Proxyで終端する：

```
Client ──HTTPS──▶ Playback Proxy ──HTTP──▶ Ghost Server
                  (MITM証明書)
```

これにより：
- Ghost Serverは証明書管理が不要
- TLS処理の複雑性をProxy側に集約
- Ghost ServerはシンプルなHTTPサーバーとして実装可能

## 実装計画

### Phase 1: Ghost Server基盤

**目標**: タイミング制御付きHTTPサーバーの基盤を構築

1. **モジュール構造**
   ```
   src/
   ├── ghost_server/
   │   ├── mod.rs           # エントリポイント
   │   ├── server.rs        # Hyper HTTPサーバー
   │   ├── handler.rs       # リクエストハンドラ
   │   ├── matcher.rs       # トランザクションマッチング
   │   └── timing.rs        # タイミング制御
   ```

2. **基本機能**
   - HTTPサーバーの起動・停止（グレースフルシャットダウン対応）
   - Hostヘッダによるバーチャルホスト処理
   - 静的レスポンスの返却（タイミング制御なし）

3. **テスト**
   - ユニットテスト: マッチングロジック
   - 統合テスト: サーバー起動・レスポンス確認

### Phase 2: タイミング制御

**目標**: 録画時のタイミングを正確に再現

1. **TTFB制御**
   - トランザクションのTTFB値に基づいて最初のバイトを遅延
   - グローバル開始時刻からの相対時間で制御

2. **チャンク送信制御**
   - `target_time`に基づいてチャンクを送信
   - 転送レートのシミュレーション

3. **テスト**
   - タイミング精度テスト（10%許容誤差）
   - 同時リクエストでのタイミング独立性

### Phase 3: Playback Proxyリファクタリング

**目標**: シンプルなリバースプロキシへの変更

1. **既存コードの整理**
   - `hudsucker_handler.rs`からレスポンス生成ロジックを削除
   - Ghost Serverへの転送ロジックに置き換え

2. **プロキシ動作**
   - リクエストのHost保持
   - Ghost Server URLへの転送
   - レスポンスのパススルー

3. **HTTPS処理**
   - 既存のMITM機能を維持
   - 復号後のリクエストをGhost Serverに転送

### Phase 4: 統合と最適化

**目標**: 両コンポーネントの統合と最終調整

1. **起動フロー**
   - Ghost Serverを先に起動
   - Playback ProxyがGhost Serverに依存
   - 両者の協調シャットダウン

2. **エラーハンドリング**
   - Ghost Server接続失敗時の処理
   - トランザクション未マッチ時の404

3. **パフォーマンス最適化**
   - 接続の再利用
   - バッファリング調整

## ファイル変更予定

### 新規作成

```
src/ghost_server/
├── mod.rs
├── server.rs
├── handler.rs
├── matcher.rs
├── timing.rs
└── tests.rs
```

### 変更

```
src/playback/
├── mod.rs              # Ghost Server起動を追加
├── proxy.rs            # リバースプロキシに簡素化
└── hudsucker_handler.rs # Ghost Server転送に変更

src/cli.rs              # ghost-server-port オプション追加（オプショナル）
```

### 削除または非推奨

```
src/playback/transaction.rs  # Ghost Serverに移動
```

## マイルストーン

| Phase | 内容 | 完了基準 |
|-------|------|----------|
| 1 | Ghost Server基盤 | HTTPサーバーが起動し、静的レスポンスを返却できる |
| 2 | タイミング制御 | 既存のacceptanceテスト（minimum timing）がパスする |
| 3 | Proxyリファクタリング | Playback ProxyがGhost Server経由で動作する |
| 4 | 統合・最適化 | 全テストがパスし、CI greenになる |

## 技術的考慮事項

### フレームワーク選択

Ghost Serverには以下の選択肢がある：

1. **Hyper直接使用**（推奨）
   - 既存の依存関係で対応可能
   - 低レベル制御が可能
   - タイミング制御に最適

2. **Axum使用**
   - 高レベルAPI
   - ルーティングが簡潔
   - 追加依存

現時点では**Hyper直接使用**を推奨。既存のplaybackコードがHyperベースであり、移行コストが低い。

### ポート設計

```
デフォルト:
  Playback Proxy: 18080（既存と同じ）
  Ghost Server: 18081（内部使用）

オプション:
  --ghost-server-port <port>  # Ghost Serverポートを明示指定
```

Ghost Serverは外部公開の必要がないため、localhostバインドのみで良い。

### 互換性

- CLIインターフェースは変更なし
- 既存のinventoryフォーマットはそのまま使用
- Go/TypeScriptラッパーへの影響なし

## リスクと対策

| リスク | 影響 | 対策 |
|--------|------|------|
| 2プロセス管理の複雑化 | 起動・停止が複雑に | 単一プロセス内で両サーバーを起動 |
| 追加のポート使用 | ポート競合 | 動的ポート探索を使用 |
| HTTPオーバーヘッド | レイテンシ増加 | localhost通信のため無視できるレベル |
| テスト工数 | 開発期間延長 | 既存テストの再利用 |

## テスト要件（死守事項）

**詳細なテストケースとアサーションは [`ghost-server-test.md`](./ghost-server-test.md) を参照。**

以下のテスト要件は現在のplaybackモードで達成されており、Ghost Serverアーキテクチャでも**必ず維持する**。
実装中は常に `ghost-server-test.md` のアサーションと照合し、すべてのテストがパスすることを確認すること。

### 1. Unit Tests (`cargo test`)

**現状**: 76個以上のユニットテスト

**必須要件**:
- すべての既存ユニットテストがパス
- 新規コンポーネント（ghost_server/）にもユニットテストを追加

### 2. Integration Tests (`tests/integration_test.rs`)

**テストケース**:

| テスト名 | 検証内容 |
|---------|---------|
| `test_recording_and_playback_integration` | Recording→Playbackの完全サイクル |
| `test_recording_error_responses` | 404/500エラーレスポンスの記録と再生 |
| `test_recording_with_compression` | gzip/brotli圧縮コンテンツの記録と再生 |
| `test_inventory_structure_validation` | Inventory JSONの構造検証 |

**必須要件**:
- HTML/CSS/JavaScript コンテンツが正しく記録・再生される
- **空白正規化後**のコンテンツが一致する（beautify処理のため完全一致ではない）
- HTTPステータスコード（200/404/500）が保存・再現される
- Content-Encoding（gzip/br）が保存・再現される
- Inventory構造が正しい（method, url, ttfbMs, statusCode 必須）

### 3. Acceptance Tests (`acceptance/`)

**Go/TypeScriptラッパーテスト**:

```
acceptance/
├── golang/main_test.go      # Goラッパー動作確認
└── typescript/              # TypeScriptラッパー動作確認
```

**必須要件**:
- **オフライン再生の証明**: HTTPサーバー停止後にPlaybackが動作すること
  ```go
  // CRITICAL: Stop the HTTP server to prove offline replay capability
  server.Close()
  // ... playback must work without origin server
  ```
- Recording/Playbackのライフサイクル管理
- シグナルベースのシャットダウン（SIGTERM/SIGINT）

### 4. E2E Tests (`e2e/`)

#### 4.1 Minimum Timing Test (`e2e/minimum/`)

**目的**: タイミング精度の検証

**6つのテストシナリオ**:

| シナリオ | ファイルサイズ | TTFB | 転送時間 |
|---------|--------------|------|---------|
| 500kb-fast | 500KB | 100ms | 200ms |
| 500kb-medium | 500KB | 500ms | 1000ms |
| 500kb-slow | 500KB | 1000ms | 2000ms |
| 1kb-fast | 1KB | 100ms | 100ms |
| 1kb-medium | 1KB | 500ms | 200ms |
| 1kb-slow | 1KB | 1000ms | 400ms |

**必須要件（10%許容誤差）**:

```rust
let tolerance = 0.10; // 10%

// Recording時の検証
fn verify_inventory(inventory_dir, scenario, tolerance) {
    // TTFB: recorded vs expected
    let ttfb_diff_ratio = ((recorded_ttfb_ms - expected_ttfb_ms).abs() / expected_ttfb_ms).abs();
    assert!(ttfb_diff_ratio <= tolerance);

    // Transfer Duration: recorded vs expected
    let transfer_diff_ratio = ((recorded_duration_ms - expected_duration_ms).abs() / expected_duration_ms).abs();
    assert!(transfer_diff_ratio <= tolerance);
}

// Playback時の検証
fn verify_timing(measured, expected_ttfb_ms, expected_total_ms, tolerance) {
    // TTFB: measured vs recorded
    let ttfb_diff_ratio = ((measured.ttfb_ms - expected_ttfb_ms).abs() / expected_ttfb_ms).abs();
    assert!(ttfb_diff_ratio <= tolerance);

    // Total: measured vs recorded
    let total_diff_ratio = ((measured.total_ms - expected_total_ms).abs() / expected_total_ms).abs();
    assert!(total_diff_ratio <= tolerance);
}
```

**検証フロー**:
1. Mock HTTPサーバーがTTFB待機+チャンク送信でレスポンス
2. Recording Proxyが計測・記録
3. Inventoryの値がMock設定と10%以内で一致
4. Playback Proxyが再生
5. Playback時の測定値がInventory記録値と10%以内で一致

#### 4.2 Content Test (`e2e/content/`)

**目的**: コンテンツ処理の検証

**Minify/Beautify検証**:

```rust
// 必須: beautify後の行数が元の2倍以上
if html_lines < minified_html_lines * 2 {
    anyhow::bail!("HTML was not properly beautified");
}
if css_lines < minified_css_lines * 2 {
    anyhow::bail!("CSS was not properly beautified");
}
if js_lines < minified_js_lines * 2 {
    anyhow::bail!("JavaScript was not properly beautified");
}

// 必須: Inventoryにminify: trueフラグ
for resource in &inventory.resources {
    if resource.url.ends_with("/index.html") {
        assert_eq!(resource.minify, Some(true));
    }
}
```

**Charset処理検証**:

| 元のCharset | 保存時 | contentCharset | Playback時 |
|------------|-------|----------------|-----------|
| Shift_JIS | UTF-8 | "Shift_JIS" | Shift_JISに再エンコード |
| EUC-JP | UTF-8 | "EUC-JP" | EUC-JPに再エンコード |
| UTF-8 | UTF-8 | "UTF-8" | そのまま |

```rust
// 必須: charset宣言の保持
// HTMLの<meta charset>やCSSの@charsetは変更しない
if url.contains("-shiftjis.html") {
    // Content-Type header
    assert_eq!(resource.content_charset, Some("Shift_JIS".to_string()));
    // <meta charset>はそのまま
    assert!(content.contains(r#"charset="Shift_JIS""#));
}

// charset-from-content: HTTPヘッダにcharsetがない場合
// コンテンツから検出してcontentCharsetに保存
if url.contains("/charset-from-content/") {
    // HTTPヘッダにはcharsetを追加しない
    assert!(!content_type.contains("charset"));
    // でもコンテンツは元のエンコーディングで返す
}
```

**Content-Encoding検証**:

| エンコーディング | Recording時 | contentEncoding | Playback時 |
|---------------|------------|-----------------|-----------|
| gzip | デコード→保存 | "gzip" | 再圧縮して返却 |
| br (brotli) | デコード→保存 | "br" | 再圧縮して返却 |
| deflate | デコード→保存 | "deflate" | 再圧縮して返却 |

```rust
// Playback時のContent-Encodingヘッダ検証
let content_encoding = response.headers().get("content-encoding");
assert_eq!(content_encoding, Some("gzip"));
```

**コンビネーションテスト**:
- Shift_JIS + gzip
- EUC-JP + brotli

### 5. CI Check (`check-ci.sh`)

**必須コマンド**:

```bash
# 1. フォーマットチェック
cargo fmt --all -- --check

# 2. Clippy（警告をエラーとして扱う）
RUSTFLAGS="-D warnings" cargo clippy --all-targets --all-features -- -D warnings

# 3. テスト
cargo test
```

**環境変数**:
```bash
RUSTFLAGS="-D warnings"      # すべての警告をエラーに
CARGO_INCREMENTAL=0          # 増分コンパイル無効
CARGO_TERM_COLOR=always
RUST_BACKTRACE=short
```

### テスト実行コマンド一覧

```bash
# ユニットテスト
cargo test

# 統合テスト（バイナリビルド必要）
cargo build --release
cargo test --test integration_test --release

# Acceptanceテスト
cd acceptance && make test-all

# E2Eテスト
cd e2e && make test-all
# または個別に:
cd e2e/minimum && ./run.sh
cd e2e/content && ./run.sh

# CIチェック（全体）
./check-ci.sh
```

## 成功の定義

1. **安定性向上**: 連続100回のplaybackテストで失敗なし
2. **タイミング精度維持**: e2e/minimum テストがパス（10%許容誤差）
3. **コンテンツ処理維持**: e2e/content テストがパス
4. **既存テストパス**: すべてのユニット・統合テストがパス
5. **Acceptanceテストパス**: Go/TypeScriptラッパーが正常動作
6. **CI green**: check-ci.shがエラーなく完了

## 次のステップ

1. このプランのレビューと承認
2. Phase 1の実装開始
3. 各Phaseごとにレビュー・マージ

---

作成日: 2026-01-28
ブランチ: `ghost-server`
