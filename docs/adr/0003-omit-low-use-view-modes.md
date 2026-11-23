# Anchor Board / Focus Mode / Zen View / Tutorial を採用しない

Den Browser に存在した `Anchor Board`、`Focus Mode`、`Zen View`、`Tutorial Board` は、Koma では採用しない。いずれも実運用での使用頻度が低く、`chromeless canvas` 化により各 Koma 上端を極薄の 1 行タイトルバーに抑えたことで、補助 UI を隠す `Zen View` の必要性も薄れたためである。

## Considered Options

- **Den Browser と同等機能を移植する**: モード切替・状態保持・専用 Koma の実装と操作体系の複雑化を招く。
- **採用しない（採用）**: 空間操作を `flat 2-axis strip` と 3 種の `on-demand overlays` に集中させ、設計・実装を簡潔に保つ。
