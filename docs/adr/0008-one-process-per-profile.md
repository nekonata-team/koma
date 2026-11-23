# 永続 Profile ごとにプロセスを分ける

Koma は複数の永続 Profile を同時に開けるようにする。
各 Profile の Workspace は個別のプロセスで動かし、ブラウザデータ、保存レイアウト、IPC の接続先を分離する。
切り替え時に元のプロセスを閉じないため、元の Workspace の Terminal セッションは動き続ける。
同じ Profile を再度開く要求は、保存先を所有するプロセスに渡す。
Private Workspace は永続 Profile の一覧に含めない。
