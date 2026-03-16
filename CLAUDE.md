# diske — macOS Disk Usage Analyzer

## What is this?
macOS向けスタンドアロンのストレージ管理デスクトップアプリ。Squarified Treemapでファイル/ディレクトリのディスク占有量を視覚化し、大きなファイルを素早く発見・管理できる。

## Tech Stack
- **Rust** (edition 2021)
- **egui/eframe 0.31** — Immediate-mode GUI。treemapの大量矩形描画に最適
- **jwalk 0.8** — rayon並列ディレクトリ走査
- **crossbeam-channel** — スキャンスレッド↔UIスレッド間通信
- **serde/serde_json** — スキャン結果のディスクキャッシュ
- **libc** — `statvfs` でボリューム容量取得
- **trash** — macOSネイティブのゴミ箱移動
- **rfd** — ネイティブファイルダイアログ

## Architecture

```
UI Thread (eframe::App)  <── crossbeam channel ──>  Scan Thread (jwalk)
       │
       ├── app.rs          : アプリ状態管理、eframe::App実装
       ├── ui/
       │   ├── treemap_view.rs : egui Painterでtreemap描画、ホバー/クリック
       │   ├── sidebar.rs     : ボリューム容量バー、Top-N大きいアイテム、タイプ別集計
       │   ├── breadcrumbs.rs : パンくずナビゲーション
       │   ├── status_bar.rs  : スキャン進捗表示
       │   └── colors.rs      : ファイル種別→色マッピング、ディレクトリ色計算
       ├── scan/
       │   ├── fs_tree.rs     : Arena-based tree (Vec<FsNode>)
       │   ├── walker.rs      : jwalk並列スキャン、スナップショット送信
       │   └── cache.rs       : ~/.cache/diske/ にJSON保存/読込
       └── treemap/
           └── layout.rs      : Squarified treemapアルゴリズム
```

### Core Data Structure: Arena-based Tree
`FsTree` は `Vec<FsNode>` のarena。ノードは親/子をインデックスで参照。Box<Node>のポインタ追いを避けてキャッシュフレンドリー。50万ノード以上でも高速に動作する。

### Scan Thread
- jwalkで並列ディレクトリ走査
- `st_blocks * 512` で実際のディスク使用量を取得（スパースファイル対応）
- 指数的間隔(10K, 30K, 90K, 270K...)でスナップショットをUIスレッドに送信
- 完了時にComplete(FsTree)を送信

### Treemap Layout
- Squarified Treemapアルゴリズム（Bruls/Huizing/van Wijk論文ベース）
- レイアウトはnavigation/resize時に再計算、(root, viewport_size)でキャッシュ

### Context Menu
- egui Windowのクロージャ内で不変借用が生きるため、ContextAction enumで遅延実行パターンを使用
- Trash後はツリーとディスクキャッシュの両方を更新

## Build & Run

```bash
# 開発ビルド
cargo run

# リリースビルド
cargo build --release

# .appバンドル作成（ダブルクリックで起動可能）
./bundle.sh
# → target/diske.app

# テスト
cargo test
```

## Known Constraints / Gotchas
- **~/.Trash はSIP保護**: 直接スキャンできない。スキャンすると空ツリーが返る
- **日本語ファイル名**: Arial Unicode フォント (/System/Library/Fonts/Supplemental/) をランタイムロード。文字列truncationはchar単位で行うこと（byte sliceするとpanic）
- **スパースファイル**: `metadata().len()` ではなく `metadata().blocks() * 512` を使う。OrbStack等のVM仮想ディスクで論理サイズが実際の100倍以上になることがある
- **egui 0.31 API**: `rect_stroke` は4引数（StrokeKind必要）、`show_tooltip_at_pointer` は4引数（LayerId必要）
- **スナップショット頻度**: ツリー全体のcloneが重いため指数的間隔で送信。5000件ごとだと大規模スキャンで性能劣化する
- **ディスクキャッシュ**: `~/.cache/diske/{hash}.json`。パスのハッシュをファイル名に使用。`CacheEnvelope`でバージョニング
- **Trash後のアイテム復活防止**: `trashed_paths: HashSet<PathBuf>`でスキャン中にTrashしたパスを追跡し、Snapshot/Complete受信時にフィルタ

### Cache Format
- `CacheEnvelope { version, tree }` でラップ。バージョン不一致時は自動破棄
- `CACHE_VERSION = 2`（FsNodeにdescendant_count追加時にバンプ）

## What's Working (as of v0.2.0)
- ホームディレクトリ自動スキャン（起動時にキャッシュロード→バックグラウンド再スキャン）
- Squarified treemap描画（ファイル種別色分け、テキスト影付き）
- クリックでディレクトリに潜る、Escape/Backspaceで戻る
- パンくずナビゲーション
- サイドバー: ボリューム容量バー、Top-20アイテム、ファイルタイプ別サイズ集計バー
- 右クリックコンテキストメニュー: Reveal in Finder (`open -R`)、Copy Path、Move to Trash
- Move to Trash確認ダイアログ（ファイル名・サイズ・パス表示）
- ファイル名検索（treemap上でハイライト、サイドバーに結果リスト表示）
- スキャン中のリアルタイムtreemap表示
- .appバンドル作成スクリプト
- Trash済みアイテムがバックグラウンドスキャン完了後に復活しないよう`trashed_paths`で追跡

## Future Ideas
- 重複ファイル検出
- タイムマシン的な履歴比較（前回スキャンとの差分表示）
- 複数ディレクトリの並列スキャン
