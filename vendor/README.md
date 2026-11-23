# 同梱コードの出典とライセンス

- [Makepad](https://github.com/makepad/makepad/tree/506ec051) のコードを同梱している。[上流の MIT ライセンス](LICENSE-Makepad) を参照。
- `makepad-app-terminal` の VT 実装は [Ghostty](https://github.com/ghostty-org/ghostty) 由来のコードを含む。[Ghostty の MIT ライセンス](makepad-app-terminal/LICENSE-Ghostty) と [ソースの帰属表示](makepad-app-terminal/src/term/mod.rs) を参照。このクレートが宣言する [Unicode License v3](makepad-app-terminal/LICENSE-Unicode) も同梱している。
- `makepad-cef` は [Chromium Embedded Framework](https://github.com/chromiumembedded/cef) のインターフェース定義を含む。[CEF のライセンス](makepad-cef/LICENSE-CEF) を参照。CEF ランタイム自体はリポジトリに含めない。
