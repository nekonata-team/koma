# Koma

Koma は、ターミナル作業とライブ Web 閲覧を同じ空間に統合し、人間と AI エージェントが同一の作業面を共有するクロスプラットフォーム作業環境である。

## Language

### 空間階層

**Profile**:
認証状態やサイトデータを他の空間と分離して保持する独立したアイデンティティ単位。1 つの Profile が 1 つの Workspace を持つ。
_Avoid_: Account, Login

**Workspace**:
1 つの Profile に属するすべての Sheet と Drawer を束ねる最上位の作業空間。
_Avoid_: Den, Space, Studio

**Private Workspace**:
終了時に履歴・認証状態・作業状態を保持しない一時的な Workspace。他 Profile のデータを共有せず、閉じると破棄される。
_Avoid_: Incognito window, Private profile, Anonymous mode

**Sheet**:
Koma を横一列に並べる作業面。Koma がない空の状態でも存在できる。
_Avoid_: Desk, Page, Window, Deck

**Sheet Preset**:
Sheet の初期構成を再利用するためのテンプレート。適用後の Sheet は Preset から独立する。
_Avoid_: Desk Preset, Workspace template, Saved sheet

### 作業面とコンテンツ

**Koma**:
Sheet 上で並べ替えられる、Content を保持して表示する作業枠。日本語では「コマ」、英語では単数形を `Koma`、複数形を `Komas` とする。
_Avoid_: Board, Panel, Tab, Card, Pane, Slot

**Terminal Koma**:
シェルまたはコマンドのセッションを表示する Koma。Sheet を切り替えてもセッションは続く。
_Avoid_: Terminal Board, Terminal window, Terminal tab

**Web Koma**:
Web ページを表示し、独自の Web History を持つ Koma。
_Avoid_: Web Board, Browser tab

**Content**:
Koma の中身の総称。具体的な対象を指すときは Web ページや Terminal セッションと呼ぶ。
_Avoid_: Sheet

**Web History**:
Web Koma ごとに保持される、戻る・進むでたどれる閲覧履歴。
_Avoid_: Sheet Stack, Tab stack

### 退避・起点・通知

**Drawer**:
どの Sheet にも置かれていない Web 資料や Terminal セッションを一時的に保持する、Workspace 共通の退避先。
_Avoid_: Inbox, Temporary sheet, Clipboard

**Drawer Item**:
Drawer に保持されている Web 資料または Terminal セッション。Sheet に置くと Koma になる。
_Avoid_: Inbox item, Bookmark, History entry

**Recent**:
新規 Koma を開く際に最近使った入力の履歴。
_Avoid_: History, Recent Koma

**Alias**:
`[aliases]` に登録する `:<key>` 形式の入力テンプレート。固定値なら即起動し、`{}` を含む場合は `:<key> <arg>` で引数を展開する。
_Avoid_: Essential, Bookmark, Quick launch

**Notification**:
実行中の Terminal 等から発火し、ユーザーに注意を促す一時通知。選択すると発火元の Koma にフォーカスが戻る。
_Avoid_: Toast, Log

### 操作・入力モード

**Palette**:
Koma の作成や切り替え、URL の編集、コマンドの実行を一つの入力欄から行うオーバーレイ。
_Avoid_: Search bar, Address bar, Launcher

**Content Input**:
フォーカス中の Koma の Content にキー入力を直接送る通常モード。
_Avoid_: Sheet Input, Insert mode

**Koma Mode**:
Koma のフォーカス移動、並べ替え、幅変更や、Sheet の切り替えを行うモード。
_Avoid_: Normal mode, Den Mode, Command mode

**Overview**:
通常のキャンバスを縮小し、すべての Sheet とそこに配置された Koma を俯瞰して編成するモード。Content へ直接入力するにはズームインして戻る。
_Avoid_: Expose, Thumbnail grid

**Ambient Targeting**:
CLI や外部プロセスが対象の ID を指定しないとき、実行元の Koma や Profile から操作対象を決める仕組み。
_Avoid_: Implicit routing
