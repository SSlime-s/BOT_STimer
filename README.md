# BOT_STimer

traQ 上で timer を簡単に設定する BOT です

(@BOT_cellophane リスペクト)

## 使い方
BOT に何かを実行させる際は `timer` を冒頭につけるか、メンションが必須です

### タイマーを開始する
(+, add, a, set, s) のいずれか + 1w2d3h4m5s (1週 2日 3時間 4分 5秒) の形式の時間 + メッセージ (Optional) で設定できます  
#### 例:
- `@BOT_STimer set 3m カップラーメン`
- `@BOT_STimer add 1d5h そろそろ出る時間だよ 僕より`
- `@BOT_STimer + 5s`
- `timer s 3m カップラーメン`

### タイマーを削除する
(-, remove, r, delete, d) のいずれか + 該当メッセージの URL (https: 省略可) で削除できます  
#### 例:
- `@BOT_STimer remove https://q.trap.jp/messages/9a1d456f-831b-4602-93ef-6617fad90972`
- `@BOT_STimer d //q.trap.jp/messages/9a1d456f-831b-4602-93ef-6617fad90972`
- `timer - //q.trap.jp/messages/9a1d456f-831b-4602-93ef-6617fad90972`

### タイマーを一覧表示する
(list, ls, l) のいずれか で一覧表示できます。  
通常は自分が設定したタイマーのみ表示されますが、これらに続いて `-a` を記述することでユーザーを問わず表示できます  
#### 例:
- `@BOT_STimer list`
- `timer ls -a`

### チャンネルに参加させる
join を続けることでチャンネルに参加させられます。  
BOT がチャンネルに参加していると、メンションなしで `timer add ...` のようにタイマーをセットできます

### チャンネルから抜けさせる
leave を続けることでチャンネルから離脱させられます。  
残念ながらこの BOT が必要ではなくなったときに使ってください  
抜けさせたとしてもメンションをしたり、また参加させることでいつでも BOT を使うことができます