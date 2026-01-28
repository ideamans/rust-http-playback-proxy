# Ghost Server テスト要件（死守事項）

このドキュメントは、現在のplaybackモードで達成されている全テストケースとアサーションを詳細に記載する。
Ghost Serverアーキテクチャでも、これらすべてのテストを**必ずパス**させること。

---

## 1. Unit Tests: 型システム (`src/types/tests.rs`)

### 1.1 ContentEncodingType シリアライズ

```rust
#[test] test_content_encoding_serialization
```
- `ContentEncodingType::Gzip` → JSON `"gzip"` に変換される
- `ContentEncodingType::Br` → JSON `"br"` に変換される

### 1.2 ContentEncodingType デシリアライズ

```rust
#[test] test_content_encoding_deserialization
```
- JSON `"gzip"` → `ContentEncodingType::Gzip` に変換される
- JSON `"deflate"` → `ContentEncodingType::Deflate` に変換される

### 1.3 ContentEncodingType FromStr

```rust
#[test] test_content_encoding_from_str
```
- `"gzip"` → `ContentEncodingType::Gzip` (Ok)
- `"br"` → `ContentEncodingType::Br` (Ok)
- `"identity"` → `ContentEncodingType::Identity` (Ok)
- `"invalid"` → Err

### 1.4 DeviceType シリアライズ

```rust
#[test] test_device_type_serialization
```
- `DeviceType::Mobile` → JSON `"mobile"`
- `DeviceType::Desktop` → JSON `"desktop"`

### 1.5 Resource 作成

```rust
#[test] test_resource_creation
```
- `Resource::new("GET", "https://example.com")` で作成
- `resource.method == "GET"`
- `resource.url == "https://example.com"`
- `resource.ttfb_ms == 0` (初期値)
- `resource.status_code.is_none()` (初期値)
- `resource.mbps.is_none()` (初期値)

### 1.6 Resource シリアライズ

```rust
#[test] test_resource_serialization
```
- JSON出力に `"method":"GET"` を含む
- JSON出力に `"url":"https://example.com"` を含む
- JSON出力に `"statusCode":200` を含む (camelCase)
- JSON出力に `"mbps":1.5` を含む

### 1.7 Inventory 作成

```rust
#[test] test_inventory_creation
```
- `Inventory::new()` で作成
- `inventory.entry_url.is_none()` (初期値)
- `inventory.device_type.is_none()` (初期値)
- `inventory.resources.is_empty()` (初期値)

### 1.8 Inventory シリアライズ

```rust
#[test] test_inventory_serialization
```
- JSON出力に `"entryUrl"` を含む (camelCase)
- JSON出力に `"deviceType"` を含む (camelCase)
- JSON出力に `"resources"` を含む

### 1.9 Inventory デシリアライズ

```rust
#[test] test_inventory_deserialization
```
- JSONからInventoryへの変換が成功
- `inventory.entry_url == Some("https://example.com")`
- `inventory.device_type == Some(DeviceType::Mobile)`
- `inventory.resources.len() == 1`
- `resource.method == "GET"`
- `resource.url == "https://example.com"`
- `resource.ttfb_ms == 100`
- `resource.status_code == Some(200)`

### 1.10 BodyChunk 作成

```rust
#[test] test_body_chunk_creation
```
- `chunk.chunk == b"test data"`
- `chunk.target_time == 1000`

### 1.11 Transaction 作成

```rust
#[test] test_transaction_creation
```
- `transaction.method == "GET"`
- `transaction.url == "https://example.com"`
- `transaction.ttfb == 50`
- `transaction.status_code == Some(200)`
- `transaction.chunks.len() == 2`
- `transaction.target_close_time == 300`

---

## 2. Unit Tests: Playback (`src/playback/tests.rs`)

### 2.1 Inventory読み込み

```rust
#[tokio::test] test_load_inventory
```
- `index.json` からInventoryを読み込める
- `loaded_inventory.entry_url == Some("https://example.com")`
- `loaded_inventory.device_type == Some(DeviceType::Desktop)`
- `loaded_inventory.resources.len() == 1`

### 2.2 Resource→Transaction変換

```rust
#[tokio::test] test_convert_resources_to_transactions
```
- UTF-8コンテンツを持つResourceを変換
- `transactions.len() == 1`
- `transaction.method == "GET"`
- `transaction.url == "https://example.com/test"`
- `transaction.ttfb == 100`
- `transaction.status_code == Some(200)`
- `transaction.chunks` が空でない

### 2.3 ファイルからのResource変換

```rust
#[tokio::test] test_convert_resource_with_file
```
- `content_file_path` で指定されたファイルからコンテンツを読み込み
- `transaction.method == "GET"`
- `transaction.url == "https://example.com/test.txt"`
- `transaction.ttfb == 50`
- `transaction.status_code == Some(200)`
- チャンクを結合すると元のファイル内容と一致

### 2.4 チャンク作成

```rust
#[test] test_create_chunks
```
- チャンクが空でない
- **最初のチャンクの`target_time == 0`** (TTFB完了直後から開始)
- チャンクを結合すると元のコンテンツと一致
- `target_close_time > 0`

### 2.5 Minify処理

```rust
#[test] test_minify_content
```
- HTML: 圧縮後のサイズ ≤ 元のサイズ
- HTML: 二重スペース `"  "` を含まない
- CSS: 圧縮後のサイズ ≤ 元のサイズ

### 2.6 圧縮処理

```rust
#[test] test_compress_content
```
- Gzip: 圧縮結果が空でない、元と異なる
- Deflate: 圧縮結果が空でない、元と異なる
- Identity: 圧縮結果が元と同一

### 2.7 チャンクタイミング（大きなコンテンツ）

```rust
#[tokio::test] test_chunk_timing_with_delay
```
- 128KB コンテンツで複数チャンクが作成される
- `chunks.len() > 1`
- チャンクの`target_time`が単調増加
- **最初のチャンクの`target_time == 0`**
- `target_close_time >= last_chunk.target_time`
- チャンク間の遅延が 400-700ms の範囲（64KB @ 1Mbps）
- **総転送時間が期待値の10%以内**

### 2.8 チャンクタイミング計算

```rust
#[test] test_chunk_timing_calculation
```
異なる帯域幅でのテスト:
- 1 Mbps, 1KB, 100ms TTFB
- 10 Mbps, 10KB, 50ms TTFB
- 0.5 Mbps, 512B, 200ms TTFB

検証:
- **最初のチャンクの`target_time == 0`**
- **`target_close_time == expected_transfer_time`**（帯域幅から計算）

### 2.9 Brotli圧縮

```rust
#[test] test_compress_brotli_content
```
- 圧縮結果が元と異なる
- 圧縮結果のサイズ < 元のサイズ

### 2.10 Deflate圧縮

```rust
#[test] test_compress_deflate_content
```
- 圧縮結果が元と異なる
- 圧縮結果が空でない

### 2.11 非常に小さいコンテンツの圧縮

```rust
#[test] test_compress_very_small_content
```
- 2バイトの入力でGzip圧縮が成功
- Identity圧縮は元と同一

### 2.12 ゼロMbpsでのチャンク作成

```rust
#[test] test_create_chunks_with_zero_mbps
```
- エッジケースを適切に処理（OkまたはErr）

### 2.13 Mbpsなしでのチャンク作成

```rust
#[test] test_create_chunks_without_mbps
```
- `mbps == None` でもチャンク作成が成功
- チャンクが空でない

### 2.14 エラーメッセージ付きResource変換

```rust
#[tokio::test] test_convert_resource_with_error_message
```
- `error_message == Some("Connection timeout")`
- `status_code == Some(504)`

### 2.15 JavaScript Minify

```rust
#[test] test_minify_javascript_content
```
- コメント付きJSを圧縮
- 圧縮後のサイズ ≤ 元のサイズ

### 2.16 不正なJSON読み込み

```rust
#[tokio::test] test_load_inventory_invalid_json
```
- 不正なJSONでエラーを返す
- `result.is_err()`

### 2.17 ContentEncodingType全種類

```rust
#[test] test_content_encoding_all_types
```
- `"gzip"`, `"br"`, `"deflate"`, `"identity"` が成功
- 大文字小文字を区別しない（`"GZIP"`, `"Br"` も成功）
- `"unknown"`, `""` はエラー

---

## 3. Unit Tests: Playback Transaction (`src/playback/transaction_tests.rs`)

### 3.1 ファイルからのTransaction変換

```rust
#[tokio::test] test_convert_resources_to_transactions_with_file
```
- MockFileSystemでファイルコンテンツを設定
- `transactions.len() == 1`
- `transaction.method == "GET"`
- `transaction.url == "https://example.com/test.txt"`
- `transaction.ttfb == 100`
- `transaction.status_code == Some(200)`
- チャンクを結合すると元のファイル内容と一致

### 3.2 UTF-8からのTransaction変換

```rust
#[tokio::test] test_convert_resources_to_transactions_with_utf8
```
- `content_utf8` からTransaction作成
- チャンクを結合するとUTF-8文字列と一致

### 3.3 Base64からのTransaction変換

```rust
#[tokio::test] test_convert_resource_with_base64
```
- `content_base64` からTransaction作成
- チャンクを結合すると元のバイナリと一致

### 3.4 チャンクタイミング

```rust
#[test] test_create_chunks_timing
```
- 1KB @ 1Mbps でチャンク作成
- **最初のチャンクの`target_time == 0`**
- 後続チャンクの`target_time`が前のチャンクより大きい
- `target_close_time > 0`

### 3.5 HTML Minify

```rust
#[test] test_minify_html_content
```
- 圧縮後のサイズ ≤ 元のサイズ
- 二重スペースを含まない

### 3.6 CSS Minify

```rust
#[test] test_minify_css_content
```
- 圧縮後のサイズ ≤ 元のサイズ
- **改行を含まない**

### 3.7 Gzip圧縮

```rust
#[test] test_compress_gzip_content
```
- 圧縮結果が元と異なる
- 圧縮結果のサイズ < 元のサイズ

### 3.8 Identity圧縮

```rust
#[test] test_compress_identity_content
```
- 結果が元と同一

### 3.9 空コンテンツのチャンク

```rust
#[test] test_empty_content_chunks
```
- **チャンクが空**
- **`target_close_time == 0`**

### 3.10 コンテンツなしResource変換

```rust
#[tokio::test] test_convert_resource_no_content
```
- **`result.is_none()`**

### 3.11 チャンクターゲット時間

```rust
#[test] test_chunk_target_times
```
- 2KB @ 2Mbps でテスト
- **最初のチャンクの`target_time == 0`**
- ターゲット時間が単調増加
- `target_close_time >= last_chunk.target_time`

### 3.12 UTF-8への再エンコード

```rust
#[test] test_re_encode_to_charset_utf8
```
- UTF-8→UTF-8 は変化なし

### 3.13 Shift_JISへの再エンコード

```rust
#[test] test_re_encode_to_charset_shift_jis
```
- UTF-8 "テスト" → Shift_JIS バイト列
- 結果がUTF-8と異なる
- Shift_JISとしてデコードすると "テスト" になる

### 3.14 Charset付きResource変換

```rust
#[tokio::test] test_convert_resource_with_content_charset
```
- `content_charset = "Shift_JIS"` のResource
- チャンクがUTF-8と異なる（Shift_JISエンコード）
- Shift_JISとしてデコードすると元の日本語になる
- **Content-Typeヘッダに "Shift_JIS" を含む**

---

## 4. Unit Tests: Playback Inventory (`src/playback/inventory_tests.rs`)

### 4.1 保存と読み込み

```rust
#[tokio::test] test_save_and_load_inventory
```
- 保存後にファイルが存在
- 読み込んだデータが一致:
  - `entry_url == Some("https://example.com")`
  - `device_type == Some(DeviceType::Desktop)`
  - `resources.len() == 1`
  - リソースの全フィールドが一致

### 4.2 ファイル未存在時の読み込み

```rust
#[tokio::test] test_load_inventory_file_not_found
```
- `result.is_err()`

### 4.3 ディレクトリ自動作成

```rust
#[tokio::test] test_save_inventory_creates_directory
```
- 存在しないディレクトリへの保存が成功
- ファイルが作成される

### 4.4 シリアライズフォーマット

```rust
#[tokio::test] test_inventory_serialization_format
```
JSON構造の検証:
- `"entryUrl"` を含む
- `"deviceType"` を含む
- `"resources"` を含む
- 値が正しい（`"mobile"`, `"POST"`, `201`, `75`）
- **2スペースインデント**: `"{\n  \"entryUrl\""` を含む

### 4.5 空Inventory

```rust
#[tokio::test] test_empty_inventory
```
- 空Inventoryの保存・読み込みが成功
- `entry_url.is_none()`
- `device_type.is_none()`
- `resources.is_empty()`

### 4.6 複雑なResource

```rust
#[tokio::test] test_inventory_with_complex_resource
```
全フィールドが正しく保存・読み込み:
- `method == "PUT"`
- `url == "https://api.example.com/data?id=123"`
- `status_code == Some(204)`
- `ttfb_ms == 300`
- `mbps == Some(0.5)`
- `error_message == Some("Rate limited")`
- `raw_headers.is_some()`
- `content_encoding == Some(ContentEncodingType::Gzip)`
- `minify == Some(true)`

### 4.7 JSONインデント

```rust
#[tokio::test] test_json_indentation_format
```
- `"{\n  \"entryUrl\""` を含む（2スペース）
- `"  \"deviceType\""` を含む
- `"  \"resources\""` を含む
- `"    \"method\""` を含む（4スペース = 2レベル）
- `"    \"url\""` を含む
- **4スペースインデントでない**: `"{\n    \"entryUrl\""` を含まない

---

## 5. Unit Tests: Recording (`src/recording/tests.rs`)

### 5.1 Processor作成

```rust
#[tokio::test] test_processor_creation
```
- RequestProcessorが正常に作成される

### 5.2 Inventory保存

```rust
#[tokio::test] test_save_inventory
```
- `index.json` ファイルが作成される
- 読み込んだInventoryが一致:
  - `entry_url == Some("https://example.com")`
  - `device_type == Some(DeviceType::Mobile)`
  - `resources.len() == 1`

### 5.3 プロキシリクエスト処理

```rust
#[test] test_handle_proxy_request_creation
```
- `resource.method == "GET"`
- `resource.url == "https://example.com"`
- `resource.ttfb_ms == 0`

### 5.4 ContentEncodingパース

```rust
#[test] test_content_encoding_parsing
```
- `"gzip"` → `ContentEncodingType::Gzip`
- `"br"` → `ContentEncodingType::Br`
- `"deflate"` → `ContentEncodingType::Deflate`
- `"identity"` → `ContentEncodingType::Identity`
- **大文字小文字を区別しない**: `"GZIP"` → `ContentEncodingType::Gzip`
- 不正な値でエラー

---

## 6. Unit Tests: Recording Processor (`src/recording/processor_tests.rs`)

### 6.1 HTMLレスポンスボディ処理

```rust
#[tokio::test] test_process_response_body_html
```
- `content_type_mime == Some("text/html")`
- `content_charset == Some("utf-8")`
- `content_file_path.is_some()`
- `minify.is_some()`

### 6.2 テキストリソース処理

```rust
#[tokio::test] test_process_text_resource
```
- ファイルが書き込まれる
- `content_file_path.is_some()`

### 6.3 バイナリリソース処理

```rust
#[tokio::test] test_process_binary_resource
```
- ファイルが書き込まれる
- `content_file_path.is_some()`
- `content_base64.is_some()`

### 6.4 Gzip解凍

```rust
#[tokio::test] test_decompress_gzip
```
- Gzip圧縮データを解凍
- 結果が元のデータと一致

### 6.5 UTF-8変換

```rust
#[test] test_convert_to_utf8
```
- `result == "Hello, 世界!"`
- `encoding_name == "UTF-8"`

### 6.6 HTML Beautify

```rust
#[test] test_beautify_html
```
- **beautify後の行数 > 元の行数**

### 6.7 CSS Beautify

```rust
#[test] test_beautify_css
```
- 改行を含む
- 長さ >= 元の長さ

### 6.8 元のCharset保持

```rust
#[tokio::test] test_original_charset_preservation
```
- `content_charset == Some("Shift_JIS")`
- **ファイル内のcharset宣言が保持される**（UTF-8に変換されない）
  - `charset="Shift_JIS"` または `charset='Shift_JIS'` または `charset=Shift_JIS` を含む

---

## 7. Unit Tests: Beautify (`src/beautify.rs`)

### 7.1 JavaScript フォーマット（シンプル）

```rust
#[test] test_format_javascript_simple
```
- minified JS をフォーマット
- **行数 > 1**
- `"function test()"` を含む

### 7.2 CSS フォーマット（シンプル）

```rust
#[test] test_format_css_simple
```
- minified CSS をフォーマット
- **行数 > 1**
- `"body"` を含む

### 7.3 HTML フォーマット（シンプル）

```rust
#[test] test_format_html_simple
```
- minified HTML をフォーマット
- **行数 > 5**
- `"  <head>"` を含む（2スペースインデント）

### 7.4 JavaScript フォーマット（複雑）

```rust
#[test] test_format_javascript_complex
```
- if/else を含む複雑な JS をフォーマット
- **行数 >= 3**

### 7.5 CSS メディアクエリ付きフォーマット

```rust
#[test] test_format_css_with_media_query
```
- `@media` ルールを含む CSS をフォーマット
- `"@media"` を含む
- `"body"` を含む

### 7.6 HTML 属性付きフォーマット

```rust
#[test] test_format_html_with_attributes
```
- `id`, `class`, `data-*` 属性を含む HTML をフォーマット
- `id="test"` が保持される

---

## 8. Unit Tests: Utils (`src/utils/tests.rs`)

### 8.1 利用可能ポート検索

```rust
#[test] test_find_available_port
```
- `port >= 18080`

### 8.2 ポート取得またはデフォルト

```rust
#[test] test_get_port_or_default
```
- `Some(9090)` → `9090`
- `None` → `port >= 18080`

### 8.3 URLからファイルパス生成（シンプル）

```rust
#[test] test_generate_file_path_from_url_simple
```
- `"https://example.com/"` → `"get/https/example.com/index.html"`

### 8.4 URLからファイルパス生成（パス付き）

```rust
#[test] test_generate_file_path_from_url_with_path
```
- `"https://example.com/path/to/resource.js"` → `"get/https/example.com/path/to/resource.js"`

### 8.5 URLからファイルパス生成（短いクエリ）

```rust
#[test] test_generate_file_path_from_url_with_short_query
```
- `"?param=value"` → `"~param%3Dvalue"` として追加

### 8.6 URLからファイルパス生成（長いクエリ）

```rust
#[test] test_generate_file_path_from_url_with_long_query
```
- 40文字以上のクエリでハッシュが使用される
- `".~"` を含む

### 8.7 URLからファイルパス生成（拡張子付き）

```rust
#[test] test_generate_file_path_from_url_with_extension
```
- `"script.js?v=1"` → `"script~v%3D1.js"` （拡張子を保持）

### 8.8 テキストリソース判定

```rust
#[test] test_is_text_resource
```
テキスト:
- `"text/html; charset=utf-8"` → true
- `"text/css"` → true
- `"application/javascript"` → true
- `"text/javascript"` → true

非テキスト:
- `"image/png"` → false
- `"application/octet-stream"` → false

### 8.9 Content-Typeからcharset抽出

```rust
#[test] test_extract_charset_from_content_type
```
- `"text/html; charset=utf-8"` → `Some("utf-8")`
- `"text/html; charset=\"utf-8\""` → `Some("utf-8")`（クォート付き）
- `"text/html; charset=shift_jis; boundary=something"` → `Some("shift_jis")`
- `"text/html"` → `None`
- `"application/json"` → `None`

### 8.10 クエリハッシュ境界

```rust
#[test] test_generate_file_path_query_hash
```
- **32文字ちょうど**: ハッシュなし
- **33文字以上**: ハッシュあり（`".~"` を含む）

### 8.11 複数クエリパラメータ

```rust
#[test] test_generate_file_path_multiple_query_params
```
- `"?q=rust&page=1&sort=date"` → `"~"` と `"q%3Drust"` を含む

### 8.12 特殊文字

```rust
#[test] test_generate_file_path_special_chars
```
- スペース付きパスが処理される
- 日本語パスが処理される

### 8.13 HTTPメソッド

```rust
#[test] test_generate_file_path_methods
```
- GET → `"get/..."`
- POST → `"post/..."`
- DELETE → `"delete/..."`

### 8.14 拡張テキストリソース判定

```rust
#[test] test_is_text_resource_extended
```
テキスト:
- `"text/html"`, `"text/css"`, `"application/javascript"`, `"text/javascript"`
- charset付きも対応

非テキスト（明示的に非対応）:
- `"text/plain"`, `"application/json"`, `"application/xml"`
- 画像、動画、音声、PDF、ZIP

### 8.15 charsetエッジケース

```rust
#[test] test_extract_charset_edge_cases
```
- 大文字 `"CHARSET=UTF-8"` → `Some("UTF-8")`
- 混合ケース `"Charset=ISO-8859-1"` → `Some("ISO-8859-1")`
- クォート付き → クォートなしで返す
- 複数パラメータでも正しく抽出
- **値の大文字小文字を保持**: `"Shift_JIS"` → `Some("Shift_JIS")`

### 8.16 HTMLからcharset抽出

```rust
#[test] test_extract_charset_from_html_meta_charset
```
- `<meta charset="UTF-8">` → `Some("utf-8")`
- `<meta charset='Shift_JIS'>` → `Some("shift_jis")`
- `<meta charset=EUC-JP>` → `Some("euc-jp")`（クォートなし）

### 8.17 HTML http-equivからcharset抽出

```rust
#[test] test_extract_charset_from_html_http_equiv
```
- `<meta http-equiv="Content-Type" content="text/html; charset=UTF-8">` → `Some("utf-8")`
- Shift_JIS版も同様

### 8.18 charsetなしHTML

```rust
#[test] test_extract_charset_from_html_no_charset
```
- charsetなし → `None`

### 8.19 HTML charset大文字小文字

```rust
#[test] test_extract_charset_from_html_case_insensitive
```
- `<HTML><HEAD><META CHARSET="UTF-8">` → `Some("utf-8")`

### 8.20 CSS @charsetダブルクォート

```rust
#[test] test_extract_charset_from_css_double_quotes
```
- `@charset "UTF-8";` → `Some("utf-8")`
- `@charset "Shift_JIS";` → `Some("shift_jis")`

### 8.21 CSS @charsetシングルクォート

```rust
#[test] test_extract_charset_from_css_single_quotes
```
- `@charset 'UTF-8';` → `Some("utf-8")`

### 8.22 CSS @charset空白付き

```rust
#[test] test_extract_charset_from_css_with_whitespace
```
- `@charset  "UTF-8"  ;` → `Some("utf-8")`

### 8.23 CSS @charsetなし

```rust
#[test] test_extract_charset_from_css_no_charset
```
- `@charset`なし → `None`

### 8.24 CSS @charset大文字小文字

```rust
#[test] test_extract_charset_from_css_case_insensitive
```
- `@CHARSET "UTF-8";` → `Some("utf-8")`

---

## 9. Integration Tests (`tests/integration_test.rs`)

### 9.1 Recording→Playback統合テスト

```rust
#[tokio::test] test_recording_and_playback_integration
```

**Recording Phase:**
1. 静的Webサーバーを起動
2. Recording Proxyを起動
3. プロキシ経由でリクエスト:
   - `index.html` → `response.status().is_success()`
   - `style.css` → `response.status().is_success()`, `"font-family"` を含む
   - `script.js` → `response.status().is_success()`, `"console.log"` を含む
4. Recording Proxyを**SIGINT**で停止

**Verification Phase:**
- `index.json` が存在
- `contents/` ディレクトリが存在
- Inventoryが3リソース以上
- 各リソースに必須フィールド: `method`, `url`, `ttfbMs`, `statusCode`

**Playback Phase:**
1. Playback Proxyを起動（静的サーバーは停止済み）
2. プロキシ経由でリクエスト:
   - `index.html` → `response.status().is_success()`
   - **空白正規化後のコンテンツが一致**
   - CSS, JSも同様

### 9.2 エラーレスポンステスト

```rust
#[tokio::test] test_recording_error_responses
```

**Recording:**
- `/not-found` → `StatusCode::NOT_FOUND`, `"404 Not Found"` を含む
- `/server-error` → `StatusCode::INTERNAL_SERVER_ERROR`, `"500 Internal Server Error"` を含む

**Inventory検証:**
- `statusCode == 404` のリソースが存在
- `statusCode == 500` のリソースが存在

**Playback検証:**
- 404レスポンスが正しく再生
- 500レスポンスが正しく再生

### 9.3 圧縮テスト

```rust
#[tokio::test] test_recording_with_compression
```

**Recording:**
- `/compressed.txt` (gzip) → 成功
- `/compressed-br.txt` (brotli) → 成功

**Inventory検証:**
- gzipリソース: `contentEncoding == "gzip"`
- brotliリソース: `contentEncoding == "br"`

**Playback検証:**
- gzipコンテンツの再生が成功
- brotliコンテンツの再生が成功

### 9.4 Inventory構造検証

```rust
#[tokio::test] test_inventory_structure_validation
```

**トップレベル構造:**
- Inventoryがオブジェクト
- `deviceType` が文字列で `"desktop"`

**リソース検証:**
- `resources` が空でない配列
- 各リソースに必須フィールド:
  - `method` が文字列
  - `url` が文字列
  - `ttfbMs` が数値、**非負**
  - `statusCode` が数値、**100-599の範囲**
- **`contentFilePath`, `contentUtf8`, `contentBase64` のいずれかが存在**

---

## 10. E2E Tests: Minimum Timing (`e2e/minimum/`)

### 10.1 テストシナリオ

| シナリオ | ファイルサイズ | TTFB | 転送時間 |
|---------|--------------|------|---------|
| 500kb-fast | 500KB | 100ms | 200ms |
| 500kb-medium | 500KB | 500ms | 1000ms |
| 500kb-slow | 500KB | 1000ms | 2000ms |
| 1kb-fast | 1KB | 100ms | 100ms |
| 1kb-medium | 1KB | 500ms | 200ms |
| 1kb-slow | 1KB | 1000ms | 400ms |

### 10.2 Recording検証

```rust
fn verify_inventory(inventory_dir, scenario, tolerance=0.10)
```

**TTFBの検証:**
- `recorded_ttfb_ms` と `expected_ttfb_ms` の差が **10%以内**

**転送時間の検証:**
- `recorded_duration_ms` と `expected_transfer_duration_ms` の差が **10%以内**

### 10.3 Playback検証

```rust
fn verify_timing(measured, expected_ttfb_ms, expected_total_ms, tolerance=0.10)
```

**TTFB検証:**
- `measured.ttfb_ms` と `recorded_ttfb_ms` の差が **10%以内**

**総時間検証:**
- `measured.total_ms` と `(recorded_ttfb_ms + recorded_duration_ms)` の差が **10%以内**

---

## 11. E2E Tests: Content (`e2e/content/`)

### 11.1 Beautify検証

```rust
fn verify_beautified_content(inventory_dir)
```

**HTML:**
- `beautified_lines >= minified_lines * 2`

**CSS:**
- `beautified_lines >= minified_lines * 2`

**JavaScript:**
- `beautified_lines >= minified_lines * 2`

### 11.2 Minifyフラグ検証

```rust
fn verify_inventory_minify_flags(inventory_dir)
```

- `/index.html` → `minify == Some(true)`
- `/style.css` → `minify == Some(true)`
- `/script.js` → `minify == Some(true)`

### 11.3 Charset検証

```rust
fn verify_charset_in_inventory(inventory_dir)
```

**/charset/ リソース（HTTPヘッダにcharset）:**
- `-shiftjis.` → `contentCharset == Some("Shift_JIS")`
- `-eucjp.` → `contentCharset == Some("EUC-JP")`
- `-utf8.` → `contentCharset == Some("UTF-8")`
- ファイルがUTF-8として読み取り可能
- **HTML: charset宣言が保持される**（`charset="Shift_JIS"` など）
- **CSS: @charset宣言が保持される**（`@charset "Shift_JIS"` など）

**/charset-from-content/ リソース（HTTPヘッダにcharsetなし）:**
- `-shiftjis.` → `contentCharset == Some("shift_jis")`（小文字）
- `-eucjp.` → `contentCharset == Some("euc-jp")`（小文字）
- `-utf8.` → `contentCharset == Some("utf-8")`（小文字）
- ファイルがUTF-8として読み取り可能

### 11.4 Content-Encoding検証

- `/encoding/gzip.html` → `contentEncoding == Some("gzip")`
- `/encoding/br.html` → `contentEncoding == Some("br")`
- `/encoding/deflate.html` → `contentEncoding == Some("deflate")`

### 11.5 Playback検証

```rust
fn verify_playback_proxy(...)
```

**Shift_JIS Charset:**
- レスポンスの`Content-Type`に `"Shift_JIS"` を含む
- ボディがShift_JISとして有効（デコードエラーなし）
- **meta tagが変更されていない**（`charset="Shift_JIS"` を含む）

**Gzip Encoding:**
- `Content-Encoding == "gzip"`

**Charset-from-content:**
- **HTTPヘッダにcharsetを追加しない**（元になかった場合）
- ボディはShift_JISエンコード
- `<meta charset>` が保持される

**CSS charset-from-content:**
- HTTPヘッダにcharsetなし
- ボディはShift_JISエンコード
- `@charset "Shift_JIS"` が保持される

---

## まとめ

**テストカウント:**
- Unit Tests: 95テスト
  - 型システム: 11テスト
  - Playback: 17テスト
  - Playback Transaction: 14テスト
  - Playback Inventory: 7テスト
  - Recording: 5テスト
  - Recording Processor: 8テスト
  - Beautify: 6テスト
  - Utils: 24テスト
  - (その他: 3テスト)
- Integration Tests: 4テスト
- E2E Tests: 2スイート（minimum: 6シナリオ、content: 5検証）

**重要な死守事項:**

1. **タイミング精度**: 10%許容誤差（TTFB、転送時間）
2. **チャンクタイミング**: 最初のチャンクは`target_time == 0`
3. **Beautify**: 行数が2倍以上に増加
4. **Charset保持**: 宣言を変更しない、Playback時に正しく再エンコード
5. **Content-Encoding**: 正しく保存・再圧縮
6. **Inventory構造**: camelCase、2スペースインデント
7. **オフライン再生**: 元サーバー停止後もPlayback動作

---

作成日: 2026-01-28
