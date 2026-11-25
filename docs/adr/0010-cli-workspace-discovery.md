# CLI で起動中の Workspace を見つける

[永続 Profile ごとにプロセスを分ける](0008-one-process-per-profile.md) 構成では、保存済みの Profile でも、その Workspace が開いて IPC を受け付けている間だけ CLI から操作できる。
Private Workspace は複数同時に開けるため、操作したい接続先を個別に選ぶ必要がある。

`koma workspace list` は起動中で IPC を受け付けている Workspace を列挙し、各 Workspace の接続先を返す。
利用者は一覧から永続 Profile の Workspace や Private Workspace を選び、その接続先を `koma --socket` に渡す。
一覧取得後に Workspace が閉じた場合は、操作時に接続エラーを返す。

`--profile` は、同じ永続 Profile を名前で繰り返し操作する場合の短縮形として、その用途が増えた時点で再検討する。
