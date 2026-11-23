# Drawer に Peek（一時表示）を設けない

Koma の Drawer には、Item を Sheet に配置せずに中身を確認・操作するための Peek（閲覧枠や一時 Floating 表示）を設けない。確認や操作をしたい場合は `Enter` で現在の Sheet に配置し、不要になったら `Ctrl+,` → `a` で Drawer へ戻す運用に一本化する。

## Considered Options

- **Drawer 内に閲覧専用の Peek を設ける**: スクロールや入力ができないため中途半端になりやすく、配置して確認する運用と比べて実用上の利点が薄い。
- **操作可能な Floating Peek（Scratchpad）を設ける**: 実用性は上がるが、水平ストリップ外に Floating 概念とフォーカス・脱出キー管理が増えて設計が複雑化する。
- **Peek を設けない（採用）**: 水平ストリップ単一の空間モデルを保ち、Drawer の操作と実装を簡潔に維持する。
