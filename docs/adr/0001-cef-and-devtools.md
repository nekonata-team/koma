# Web エンジンに CEF を採用し、Inspection を DevTools へ委譲する

macOS / Linux / Windows で一貫したブラウザ統合環境を提供するため、OS 固有の `WKWebView` ではなく **CEF** を採用した。また、Web 要素調査用の独自 `Inspection Koma` は構築せず、CEF 標準の DevTools を利用する方針とした。

## Considered Options

- **`WKWebView` + 独自 `Inspection Koma`**: macOS での省メモリ・ネイティブ統合に優れるが、Linux / Windows への展開とインスペクタの独自実装・保守コストが大きい。
- **CEF + DevTools 委譲（採用）**: メモリ消費は増えるものの、クロスプラットフォームで同一のレンダリング・JS 実行・DevTools 基盤を利用できる。
