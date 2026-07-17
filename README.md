# dwt

番号付きの worktree 開発コンテナへ `docker exec` するラッパーコマンド
（**d**ocker **w**ork**t**ree）。

```
dwt <N> <command> [args...]
```

は設定ファイルのテンプレートを使って以下に展開される（プロセスは exec で置き換え）:

```
docker exec -w <workdir の {n} を N に置換> <container の {n} を N に置換> <command> [args...]
```

例（`container = myproj-wt-{n}-app-1`, `workdir = /workspaces/worktree-{n}` の場合）:

```bash
dwt 2 git status
# → docker exec -w /workspaces/worktree-2 myproj-wt-2-app-1 git status
```

## 目的

devcontainer 内から作成された git worktree は gitdir がコンテナ内パスを指すため、
ホストでは git やビルドツールが直接動かない。毎回 `docker exec -w ... <container> ...`
を打つのは冗長で、コーディングエージェント（Claude Code 等）から使う場合は
コマンド形状が揺れるたびに許可プロンプトが出る。

`dwt` はインターフェースを固定することでこの揺れをなくし、
エージェント側は `Bash(dwt:*)` のような 1 ルールで安全に自動許可できるようにする。

## 設定

設定ファイルと環境変数の両方で指定でき、**環境変数が優先**される。

環境変数: `DWT_CONTAINER` / `DWT_WORKDIR` / `DWT_ALLOWED`

設定ファイルは `~/.config/dwt/config`（`XDG_CONFIG_HOME` があればそちらを優先）に
`key = value` 形式で記述する。`#` 始まりの行はコメント。

```
container = myproj-wt-{n}-app-1
workdir = /workspaces/worktree-{n}
# allowed = git,uv,python,npm
```

| キー | 環境変数 | 必須 | 説明 |
|---|---|---|---|
| `container` | `DWT_CONTAINER` | ✓ | コンテナ名テンプレート。`{n}` が第1引数の N に置換される |
| `workdir` | `DWT_WORKDIR` | ✓ | コンテナ内 working directory テンプレート。同上 |
| `allowed` | `DWT_ALLOWED` | | コンテナへ渡すことを許可するコマンドのカンマ区切りリスト。デフォルト `git,uv,python,npm` |

## コマンド制限

安全のため、コンテナへ渡すコマンドを制限している:

- `allowed` に列挙されたコマンドのみ実行可能
- `git push` と `--force*` オプション付きの git は拒否

拒否された操作は素の `docker exec` で実行する想定（エージェントからは許可プロンプトが出る）。

## インストール

flake として公開しているので、home-manager 等から input として組み込む:

```nix
inputs.dwt.url = "github:wao3299/dwt";
```

```nix
home.packages = [ dwt-pkgs.default ];
xdg.configFile."dwt/config".text = ''
  container = myproj-wt-{n}-app-1
  workdir = /workspaces/worktree-{n}
'';
```

dwt を更新したら利用側で:

```bash
nix flake lock --update-input dwt
```

## 開発

```bash
nix develop   # cargo / rustc / clippy 入りのシェル
cargo build
```
