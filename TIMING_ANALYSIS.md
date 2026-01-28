# タイミング管理の分析レポート

## 現在のアーキテクチャ

```
Browser <-> MITM Proxy (Hudsucker) <-> GhostForwarder <-> Ghost Server
                                              |
                                    (reqwest HTTP client)
```

## 発見した潜在的問題

### 問題1: GhostForwarderのストリーミング遅延非保持

**場所**: `src/playback/ghost_forwarder.rs` (lines 179-185)

```rust
let body_stream = ghost_response
    .bytes_stream()
    .map_err(...);
let body = Body::from_stream(body_stream);
```

**問題**: Ghost Serverがチャンクタイミングを制御していますが、GhostForwarderはreqwestを使用してGhost Serverからレスポンスを受信しています。

reqwestの`bytes_stream()`は**TCPバッファから利用可能なデータをすぐに返します**。Ghost Serverがチャンク間で待機しても:

1. TCPの受信バッファにデータが蓄積される可能性
2. reqwestの内部バッファリング
3. Hudsuckerのバッファリング

これらにより、タイミング制御が失われる可能性があります。

### 問題2: 二重ホップのレイテンシ追加

GhostForwarder経由の二重ホップにより、以下の追加レイテンシが発生:

```
Ghost Server TTFB wait → reqwest receives headers → GhostForwarder builds response → Hudsucker sends to browser
```

ただし、これは127.0.0.1内の通信なので、通常は1ms未満。

### 問題3: HTTP/2フレームバッファリング

HTTP/2では、複数のストリームが多重化されます。Hudsuckerがレスポンスをバッファリングしてから送信する可能性があります。

## 検証すべき項目

### 1. 実際のTTFB値を確認

```bash
# inventoryのindex.jsonを確認
cat inventory/index.json | jq '.resources[] | {url: .url, ttfb_ms: .ttfbMs, duration_ms: .durationMs}' | head -50
```

### 2. ログでタイミングを確認

playback実行時に以下のログが出力されます:
- `Waiting Xms for TTFB` - TTFBの待機
- `Serving transaction: ... chunks=N, target_close=Xms` - チャンク数と転送時間

これらの値が期待通りか確認してください。

### 3. 単純なcurl テストでタイミング測定

```bash
# TTFBを測定
curl -w "TTFB: %{time_starttransfer}s, Total: %{time_total}s\n" \
  --proxy http://127.0.0.1:18080 \
  -o /dev/null -s \
  https://example.com/
```

## 推奨される修正

### 修正案1: GhostForwarderを廃止し、直接Ghost Serverに接続

最もシンプルで効果的な解決策:

```
Browser <-> MITM Proxy (Hudsucker) <-> Ghost Server (直接接続)
```

これにより:
- 中間のHTTPホップがなくなる
- タイミング制御がより正確になる
- バッファリングの問題が軽減される

### 修正案2: HudsuckerハンドラでGhost Serverのhyperクライアントを直接使用

reqwestの代わりにhyperを直接使用することで、より低レベルな制御が可能に:

```rust
use hyper_util::client::legacy::Client;

// hyper clientを使用してGhost Serverに接続
let connector = hyper_util::client::legacy::connect::HttpConnector::new();
let client = Client::builder(TokioExecutor::new()).build(connector);
```

### 修正案3: ストリーミング検証ログの追加

タイミングが正しく機能しているか確認するため、詳細ログを追加:

```rust
// Ghost Server: チャンク送信時にログ
info!("Sending chunk {} at {}ms (target: {}ms)", chunk_idx, elapsed, chunk.target_time);

// GhostForwarder: チャンク受信時にログ
info!("Received chunk from Ghost Server at {}ms", start.elapsed().as_millis());
```

## 現在のタイミングフロー詳細

### Recording時:
1. `request_start` = リクエスト送信時刻
2. `ttfb_instant` = `handle_response`開始時（ヘッダー到着直後）
3. `ttfb_ms` = `ttfb_instant - request_start`
4. `download_end` = `body.collect().await`完了時
5. `duration_ms` = `download_end - ttfb_instant`

### Playback時:
1. Ghost Server: `ttfb_ms`ミリ秒待機
2. Ghost Server: ヘッダー送信
3. Ghost Server: 各チャンクを`target_time`まで待機して送信
4. Ghost Server: `target_close_time`まで待機してコネクション終了

### 問題のある部分:
- GhostForwarder経由でのストリーミングにより、チャンクタイミングが不正確になる可能性
- reqwestがバッファリングを行う可能性
- TCPスタックのバッファリング

## 次のステップ

1. 上記の検証項目を実行して問題を特定
2. ログを追加してタイミングの実際の動作を確認
3. 修正案1（Ghost Server直接接続）の実装を検討
