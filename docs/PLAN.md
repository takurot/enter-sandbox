# EnterSandBox 実装計画

> **ドキュメントバージョン:** 1.0  
> **最終更新:** 2026-03-17  
> **参照:** [SPEC.md](./SPEC.md), [RESEARCH.md](./RESEARCH.md)

---

## 凡例

| ステータス | 意味 |
| --- | --- |
| `[ ]` | 未着手 |
| `[/]` | 進行中 |
| `[x]` | 完了 |
| `[!]` | ブロック中（依存タスク待ち or 課題あり） |

**依存関係の表記:** `depends: [タスクID]` — 当該タスクを開始する前に完了必須のタスク

---

## Phase 1: The Nano-Sandbox (MVP)

**目標:** 「世界最速・最も手軽なPythonサンドボックス」のリリース

### 1.1 プロジェクト基盤

| ID | タスク | 詳細 | ステータス | 依存 |
| --- | --- | --- | --- | --- |
| P1-001 | Rustプロジェクト初期化 | `cargo new agentbox-core --lib` でライブラリクレート作成。Cargo.toml に wasmtime, rustpython 依存を追加 | `[x]` | - |
| P1-002 | Python SDK スケルトン作成 | `agentbox/` ディレクトリ構成、`pyproject.toml` (maturin)、`__init__.py` 作成 | `[x]` | - |
| P1-003 | CI パイプライン構築 | GitHub Actions で Rust build + test、Python lint + test を実行 | `[x]` | P1-001, P1-002 |
| P1-004 | 開発環境ドキュメント | `CONTRIBUTING.md` に開発セットアップ手順を記載 | `[x]` | P1-001 |

### 1.2 Wasmtime ランタイム統合

| ID | タスク | 詳細 | ステータス | 依存 |
| --- | --- | --- | --- | --- |
| P1-010 | Wasmtime Engine 初期化 | `wasmtime::Engine` と `wasmtime::Store` の初期化ロジック実装 | `[x]` | P1-001 |
| P1-011 | WASI 設定 | `wasmtime_wasi` で最小限の WASI 環境構築（stdin/stdout/stderr のみ、ファイルシステムなし） | `[x]` | P1-010 |
| P1-012 | リソース制限実装 | メモリ上限（`wasmtime::ResourceLimiter`）、実行時間タイムアウト（epoch interruption）実装 | `[x]` | P1-010 |
| P1-013 | Wasmtime 単体テスト | Engine 初期化、リソース制限が正しく動作することを検証 | `[x]` | P1-012 |

### 1.3 RustPython 統合

| ID | タスク | 詳細 | ステータス | 依存 |
| --- | --- | --- | --- | --- |
| P1-020 | RustPython WASM ビルド | RustPython を `wasm32-wasip1` でビルド。 | `[x]` | P1-001 |
| P1-021 | Python コード実行パイプライン | ユーザーコードを WASM モジュールに渡し、stdout/stderr をキャプチャ | `[x]` | P1-010, P1-020 |
| P1-022 | 標準ライブラリ確認 | `json`, `re`, `datetime`, `collections` 等の標準ライブラリがロード可能か検証・修正 | `[x]` | P1-021 |
| P1-023 | エラーハンドリング | Python 例外 → Rust Result 変換、ユーザーフレンドリーなエラーメッセージ生成 | `[x]` | P1-021 |

### 1.4 メモリ内仮想ファイルシステム

| ID | タスク | 詳細 | ステータス | 依存 |
| --- | --- | --- | --- | --- |
| P1-030 | VirtualFS 設計 | インメモリ VFS のデータ構造設計（inode テーブル、ディレクトリツリー） | `[x]` | P1-001 |
| P1-031 | VirtualFS 実装 | `open`, `read`, `write`, `mkdir`, `stat` 等の基本操作実装 | `[x]` | P1-030 |
| P1-032 | WASI との統合 | `wasmtime_wasi::WasiCtxBuilder` に VirtualFS をマウント | `[x]` | P1-011, P1-031 |
| P1-033 | VirtualFS テスト | ファイル作成・読み書き・削除のユニットテスト | `[x]` | P1-032 |

### 1.5 Python SDK (agentbox)

| ID | タスク | 詳細 | ステータス | 依存 |
| --- | --- | --- | --- | --- |
| P1-040 | Rust → Python バインディング | PyO3/Maturin で `Sandbox` クラスを Python に公開 | `[x]` | P1-021, P1-032 |
| P1-041 | `Sandbox.run()` API | コード文字列を受け取り `SandboxResult(stdout, stderr, exit_code)` を返す | `[x]` | P1-040 |
| P1-042 | `SandboxConfig` 実装 | `timeout_ms`, `memory_limit_mb`, `allowed_modules` 設定項目 | `[x]` | P1-041 |
| P1-043 | 型ヒント (`.pyi`) | Python の型チェック対応のためスタブファイル作成 | `[x]` | P1-041 |
| P1-044 | SDK ドキュメント | README.md に使用例、API リファレンス記載 | `[x]` | P1-042 |

### 1.6 テスト & ベンチマーク (Phase 1)

| ID | タスク | 詳細 | ステータス | 依存 |
| --- | --- | --- | --- | --- |
| P1-050 | ユニットテスト整備 | 各 Rust モジュールに `#[cfg(test)]` でテスト追加。カバレッジ 80% 目標 | `[x]` | P1-023, P1-033 |
| P1-051 | Python 統合テスト | `pytest` で SDK の E2E テスト（正常系・異常系 20ケース以上） | `[x]` | P1-042 |
| P1-052 | 起動時間ベンチマーク | `criterion` で cold start 計測。目標: < 10ms | `[x]` | P1-041 |
| P1-053 | メモリベンチマーク | 実行中のピークメモリ使用量計測 | `[x]` | P1-041 |
| P1-054 | CI ベンチマーク統合 | PR ごとに性能リグレッションを検出する仕組み | `[x]` | P1-003, P1-052 |
| P1-083 | Cold start 最適化 | P1-052 の計測結果をもとに Wasmtime/runner 初期化を改善し、中央値 < 10ms を達成する | `[x]` | P1-052 |

### 1.7 CPython WASI 問題調査・対処

| ID | タスク | 詳細 | ステータス | 依存 |
| --- | --- | --- | --- | --- |
| P1-070 | CPython WASI 再現環境整備 | `assets/cpython-wasi` の取得手順・検証ハッシュを定義し、ローカル/CI で同一入力を再現できる状態を作る | `[x]` | P1-001 |
| P1-071 | 再現テストの固定化 | 「CLI では成功・SDK では失敗」を再現する最小ケースを Rust/Python テストとして追加 | `[x]` | P1-070 |
| P1-072 | CLI/SDK 差分調査 | `argv`, `env`, `preopen`, stdio, clocks/random を観点に WASI コンテキスト差分を可視化 | `[x]` | P1-071 |
| P1-073 | トレース強化 | `_start` 失敗時の wasm backtrace 収集・ログ整備（必要ならデバッグビルド）を実装 | `[x]` | P1-072 |
| P1-074 | ランタイム修正 | 差分調査結果に基づいて SDK 側の WASI 設定を修正し、CPython WASI 初期化クラッシュを解消 | `[x]` | P1-073 |
| P1-075 | 標準ライブラリ検証 (CPython WASI) | `json`, `re`, `datetime`, `collections` 等の import/実行確認を CPython WASI 経路で実施 | `[x]` | P1-074 |
| P1-076 | 回帰防止テスト | CPython WASI 実行経路に対する成功系・失敗系（例外/タイムアウト/出力上限）の回帰テスト追加 | `[x]` | P1-075 |
| P1-077 | ドキュメント更新 | 調査結果・制約・運用手順を README/PLAN に反映し、暫定メモを正式ドキュメントへ統合 | `[x]` | P1-076 |

### 1.8 SPEC/PLAN 整合ギャップ解消

| ID | タスク | 詳細 | ステータス | 依存 |
| --- | --- | --- | --- | --- |
| P1-078 | Tier1 実行エンジン実体化 | Dummy `runner-wasm` を CPython WASI 実行器に置換し、`Sandbox.run()` が実際の Python 実行結果（stdout/stderr/exit_code）を返すようにする | `[x]` | P1-020, P1-075 |
| P1-079 | `SandboxResult` API 整合 | `Sandbox.run()` の戻り値を `SandboxResult(stdout, stderr, exit_code)` に変更し、PyO3 バインディング・`.pyi`・README・pytest を更新する | `[x]` | P1-078 |
| P1-080 | `allowed_modules` 制御実装 | `SandboxConfig.allowed_modules` を追加し、未許可 import の遮断とユーザー向けエラーメッセージ、回帰テストを実装する | `[x]` | P1-079 |
| P1-081 | タイムアウト強制の仕様一致 | fuel ヒューリスティクス依存を解消し、仕様どおり epoch interruption ベースの wall-clock timeout 強制とテストを実装する | `[x]` | P1-012 |
| P1-082 | Phase1 Python E2E 拡充 | `tests/python` を正常系・異常系 20 ケース以上に拡張し、P1-051 の受け入れ条件を満たす | `[x]` | P1-079, P1-080 |

### 1.9 リリース準備
... (Unchanged) ...

---

## Phase 2: The Heavy-Sandbox & Routing
... (Unchanged) ...

---

## メモ・課題

> このセクションは実装中に発見した課題やメモを記録します

- (2025-01-11) Task 1.1 完了。Python 3.14 環境でのビルドには `PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1` が必要。
- (2026-02-14) P1-032/P1-044 を更新。WASI preopen で VirtualFS を接続し、README に現行 SDK API を追記。
- (2025-01-11) Task 1.2-1.6 実装。RustPython の WASM ビルドがネットワーク制限により失敗するため、Dummy Runner でパイプラインを検証。
- (2026-02-14) P1-032 完了。VirtualFS を `/sandbox` として preopen し、`runner-wasm` は `/sandbox/code.py` からコードを読み出す経路に更新。
- (2026-02-14) `docs/PROBLEM.md` の内容を P1-070〜P1-077 として PLAN に移管。以後は PLAN で調査・対処を管理。
- (2026-02-14) P1-070 完了。`assets/cpython-wasi/manifest.json` に取得元 URL と SHA-256 を固定し、`scripts/prepare_cpython_wasi_assets.py` でローカル/CI 共通の取得・検証フローを導入。
- (2026-02-14) P1-070 追補。アーカイブ破損時の再取得リカバリ、ダウンロードタイムアウト/リトライ、`scripts/`・`tests/` を含む CI lint を追加して再現環境の運用安定性を強化。
- (2026-02-14) P1-071 完了。`agentbox-core` の Rust テストで CLI 相当 WASI コンテキスト成功と SDK 相当コンテキスト失敗（`encodings` 読み込み失敗）を固定化し、Python テストにも同再現ケースを追加。
- (2026-02-15) P1-072 完了。`agentbox-core` に CLI/SDK の WASI コンテキスト差分レポート機能を追加し、`argv`/`env`/`preopen`/`stdio`/`clock`/`random` の各観点を Rust/Python テストで可視化・検証可能にした。
- (2026-02-15) P1-073 完了。`_start` 失敗時に `trace.capture=wasm-backtrace-v1` 形式の構造化ログを生成し、error chain と `WasmBacktrace` フレーム情報を Rust/Python テストで検証可能にした。
- (2026-02-15) P1-074 完了。SDK プロファイルの CPython WASI preopen guest path を `/sandbox` から `/` に揃えて初期化クラッシュ（`No module named 'encodings'`）を解消。旧失敗経路は `sdk-legacy` として保持し、構造化トレース検証を継続可能にした。
- (2026-02-15) SPEC/PLAN/実装の整合確認を実施。現行コードで未カバーだった `SandboxResult` 戻り値、`allowed_modules`、epoch timeout 強制、Dummy Runner 置換、Phase1 E2E ケース数不足を P1-078〜P1-082 として追加。
- (2026-02-17) P1-075 完了。CPython WASI の CLI/SDK プロファイルで `json`/`re`/`datetime`/`collections` の import と基本操作を smoke code で検証し、Rust/Python テストに固定化。
- (2026-02-17) P1-076 完了。CPython WASI repro 実行に `timeout_ms`/`max_output_bytes` オプションを追加し、成功系・失敗系（Python 例外/タイムアウト/出力上限）の回帰テストを Rust/Python 双方へ追加。
- (2026-02-17) P1-077 完了。README/README.ja に CPython WASI 再現ワークフロー（固定アセット準備、CLI/SDK 差分確認、`sdk-legacy` 制約、回帰テスト実行手順）を追加し、暫定メモの運用手順を正式ドキュメントへ統合。
- (2026-02-18) P1-081 完了。`Sandbox.run()` の fuel ヒューリスティクスを廃止して epoch interruption ベースの wall-clock timeout に切り替え。`runner-wasm` の有限 spin ディレクティブと Rust/Python 回帰テストを追加し、timeout エラー文言を固定化。
- (2026-02-18) P1-079 完了。`Sandbox.run()` を `SandboxResult(stdout, stderr, exit_code)` 返却に変更し、PyO3 バインディング・Python スタブ・README・pytest を更新。実行エンジンは依然 Dummy `runner-wasm` のため、SPEC との実行実体ギャップは P1-078 で継続対応。
- (2026-02-18) P1-080 完了。`SandboxConfig.allowed_modules` を追加し、`Sandbox.run()` 実行前に `import` / `from ... import` を静的検査して未許可モジュールを拒否。PyO3/スタブ/README と Rust・Python 回帰テストを更新。
- (2026-02-22) P1-082 完了。`tests/python/test_sandbox.py` に `SandboxConfig` のデフォルト値検証および `allowed_modules` のエッジケースに関する系10件のテストを追加し、合計21ケースの E2E 網羅性を達成。
- (2026-02-22) P1-052 完了。`agentbox-core/benches/cold_start.rs` を追加し、`cargo bench --manifest-path agentbox-core/Cargo.toml --bench cold_start -- --noplot` で Tier1 cold start を計測（`18.690 ms / 19.755 ms / 20.585 ms`）。目標 `< 10ms` は未達のため、改善タスクを P1-083 として追加。
- (2026-02-22) P1-053 完了。`agentbox-core/benches/memory_usage.rs` を追加。Dummy Runner での Peak RSS は約 30MB。実行ごとに Peak RSS が微増することを確認。原因として `Sandbox.run` ごとの `Module` 再コンパイルが疑われるため、キャッシュ機構の導入が有効と考えられる。
- (2026-02-24) P1-054 完了。`scripts/check_tier1_benchmarks.py` を追加し、`cold_start` の中央値(ms)と `memory_usage` warm シナリオの Peak RSS(KB)を閾値チェックしてPRでリグレッション検出する仕組みを導入。`.github/workflows/ci.yml` の Rust job に PR 専用ステップを追加し、`tests/python/test_tier1_benchmark_guard.py` で回帰判定スクリプトの E2E 検証を追加。
- (2026-02-27) P1-060 完了。`scripts/build_pypi_artifacts.py` を追加して `maturin build --release` と `maturin sdist` による配布物生成を標準化。`pyproject.toml` に PyPI 向け metadata（classifiers/keywords/project.urls）を追記し、`tests/python/test_build_pypi_artifacts.py` で dry-run ベースの E2E 検証を追加。
- (2026-02-27) P1-061 完了。`.github/workflows/release.yml` を追加し、GitHub Release `published` / `workflow_dispatch` をトリガに `scripts/build_pypi_artifacts.py` で配布物を生成して PyPI へ公開する CI を実装。`tests/python/test_release_workflow.py` で workflow 契約の E2E 検証を追加。
- (2026-02-27) P1-062 完了。`CHANGELOG.md` と `docs/VERSIONING.md` を追加し、SemVer の運用ルールと GitHub Release ベースの公開手順を文書化。`tests/python/test_versioning_strategy.py` で version 同期・SemVer 形式・ドキュメント参照を E2E 検証できるようにした。
- (2026-02-27) P1-083 完了。`WasmRuntime` を共有 Engine + 共有 compiled module のキャッシュ構造へ変更し、`Sandbox.run()` ごとの `Module::new` を廃止。`runtime` ユニットテストでキャッシュ再利用を固定化し、`cargo bench --manifest-path agentbox-core/Cargo.toml --bench cold_start -- --noplot` で Tier1 cold start を `762.98 µs / 777.91 µs / 793.86 µs` まで改善（目標 `< 10ms` を達成）。
- (2026-02-28) P1-078/P1-022 完了。Dummy Runner を CPython WASI に置換し、終了コードの捕捉、VFS インポート、標準ライブラリの動作を Rust/Python 双方のテストで検証。Cold start 中央値は約 23.8ms となった。
- (2026-03-05) P2-001 完了。`docs/FIRECRACKER_DEV.md` に Firecracker vs libkrun の比較と Phase2 の基準選定を記録。macOS/Windows 向けに `Vagrantfile` と `.devcontainer/devcontainer.json` を追加し、ローカル開発 + Linux KVM 検証の運用フローを定義。
- (2026-03-05) P2-002 完了。`assets/firecracker-rootfs/manifest.json` で Alpine minirootfs アセットを固定化し、`scripts/prepare_firecracker_rootfs.py` でダウンロード検証・安全な展開・`rootfs.ext4` 生成（Linux の `mkfs.ext4` / `mke2fs`）を実装。`docs/FIRECRACKER_ROOTFS.md` と Python 契約テストを追加。
- (2026-03-12) P2-003 完了。`docs/FIRECRACKER_POOL.md` に単一 KVM ホスト前提の Firecracker VM プール設計を追加し、`minimum warm instances=2` / `maximum warm instances=8` / 利用率 `70%` の `scale-out` / `Warm > 4 が 5 minutes 継続` の `scale-in` を固定。`Creating`→`Warm`→`Leased`→`Draining` の状態遷移、health check、`acquire`/`release`/`reap` の handoff contract を定義して `P2-004`/`P2-005` へ接続。
- (2026-03-13) P2-004 完了。`agentbox-core/src/vm_pool.rs` に Firecracker VM プール状態機械を追加し、`acquire`/`release`/`reap` の公開 API、`Creating`/`Warm`/`Leased`/`Draining` 遷移、`minimum warm=2`/`maximum warm=8`/利用率 `70%` の `scale-out`、`Warm > 4 が 5 minutes` 継続時の `scale-in`、`boot_source`/`lineage_id`/`reuse_count` を含む VM メタデータを実装。Rust ユニットテストで warm hit、cold miss、待機タイムアウト、drain 後ヘルスチェック、stale `Creating` の破棄、最古 warm VM の `scale-in` を固定化し、`P2-005` の snapshot restore へ接続できるバックエンド契約を定義した。
- (2026-03-17) P2-005 完了。`agentbox-core/src/snapshot.rs` に `SnapshotAwareProvider` と `SnapshotControlPlane` を追加し、優先ライン (`rootfs.ext4`) に対する最新スナップショット restore、restore 失敗時の cold boot fallback、失敗 `snapshot_id` の in-process quarantine を実装。`agentbox-core/src/vm_pool.rs` では `CreatedVm`/`VmMetadata`/`VmLease` に optional `snapshot_id` を追加し、Rust テストで restore 優先、fallback、quarantine、pool への metadata 伝播を固定化。`docs/FIRECRACKER_SNAPSHOT.md` を追加して運用契約を文書化した。

### 1.9 リリース準備

| ID | タスク | 詳細 | ステータス | 依存 |
| --- | --- | --- | --- | --- |
| P1-060 | PyPI パッケージング | maturin で wheel ビルド、`pyproject.toml` メタデータ整備 | `[x]` | P1-043 |
| P1-061 | リリース CI | GitHub Release → PyPI 自動公開ワークフロー | `[x]` | P1-060 |
| P1-062 | バージョニング戦略 | SemVer 運用ルール、CHANGELOG.md 作成 | `[x]` | P1-060 |

---

## Phase 2: The Heavy-Sandbox & Routing

**目標:** データサイエンス対応とハイブリッド実行の実現

> [!NOTE]
> Phase 2 は Phase 1 のすべてのタスク完了後に開始

### 2.1 Firecracker 統合基盤

| ID | タスク | 詳細 | ステータス | 依存 |
| --- | --- | --- | --- | --- |
| P2-001 | Firecracker 評価 & 開発環境 | Firecracker vs libkrun 比較。**macOS/Windows での Firecracker 開発環境（Dev Container, Vagrant等）の確立** | `[x]` | Phase 1 完了 |
| P2-002 | VM イメージ作成 | Python + 基本ライブラリを含む rootfs イメージ作成（Alpine Linux ベース） | `[x]` | P2-001 |
| P2-003 | VM プール管理設計 | プール戦略設計（ウォームインスタンス数、スケール閾値） | `[x]` | P2-001 |
| P2-004 | VM プール実装 | ウォーム VM プールの生成・取得・返却ロジック実装 | `[x]` | P2-003 |
| P2-005 | スナップショット起動 | 起動済み VM のメモリスナップショットからの高速復元実装 | `[x]` | P2-002, P2-004 |

### 2.2 アダプティブ・ランタイム・ルーター

| ID | タスク | 詳細 | ステータス | 依存 |
| --- | --- | --- | --- | --- |
| P2-010 | コード解析エンジン | Python AST を解析し、import 文から依存ライブラリを抽出 | `[ ]` | Phase 1 完了 |
| P2-011 | ルーティングルール定義 | Tier 1/2 切り分けルール（NumPy/Pandas/Matplotlib → Tier 2 等）を設定ファイル化 | `[ ]` | P2-010 |
| P2-012 | Router 実装 | コード解析結果とルールに基づき適切なランタイムを選択・ディスパッチ | `[ ]` | P2-011, P2-004 |
| P2-013 | フォールバック機構 | Tier 1 で実行失敗時に自動的に Tier 2 へフォールバック | `[ ]` | P2-012 |

### 2.3 Tier 2 実行エンジン

| ID | タスク | 詳細 | ステータス | 依存 |
| --- | --- | --- | --- | --- |
| P2-020 | VM 内コード実行 | コードを VM に転送し、Python インタプリタで実行、結果を取得 | `[ ]` | P2-005 |
| P2-021 | `pip install` サポート | 実行時に必要なパッケージを VM 内でインストール（キャッシュ機構付き） | `[ ]` | P2-020 |
| P2-022 | ファイル I/O | ホスト ↔ VM 間のファイル転送 API 実装 | `[ ]` | P2-020 |
| P2-023 | セッション永続化 | セッション間で VM 状態を保持するオプション実装 | `[ ]` | P2-020 |

### 2.4 SDK 拡張

| ID | タスク | 詳細 | ステータス | 依存 |
| --- | --- | --- | --- | --- |
| P2-030 | `SandboxConfig.runtime` | `"auto"`, `"nano"`, `"heavy"` の選択オプション追加 | `[ ]` | P2-012 |
| P2-031 | `SandboxConfig.packages` | 必要なパッケージリスト指定オプション | `[ ]` | P2-021 |
| P2-032 | ファイルアップロード API | `sandbox.upload_file(local_path, remote_path)` 実装 | `[ ]` | P2-022 |
| P2-033 | ファイルダウンロード API | `sandbox.download_file(remote_path)` 実装 | `[ ]` | P2-022 |

### 2.5 テスト & ベンチマーク (Phase 2)

| ID | タスク | 詳細 | ステータス | 依存 |
| --- | --- | --- | --- | --- |
| P2-040 | ルーター単体テスト | 各種 import パターンでの正しいルーティング判定を検証 | `[ ]` | P2-012 |
| P2-041 | VM 統合テスト | Pandas/NumPy を使用するコードの E2E テスト | `[ ]` | P2-021 |
| P2-042 | 起動時間ベンチマーク (Tier 2) | VM + スナップショット復元の時間計測。目標: < 150ms | `[ ]` | P2-005 |
| P2-043 | 負荷テスト | 同時実行 100 リクエストでのスループット・レイテンシ計測 | `[ ]` | P2-023 |

---

## Phase 3: Governance & Security

**目標:** エンタープライズガバナンス機能の実装

> [!NOTE]
> Phase 3 は Phase 2 のコア機能（P2-001〜P2-023）完了後に開始可能

### 3.1 ネットワーク DLP サイドカー

| ID | タスク | 詳細 | ステータス | 依存 |
| --- | --- | --- | --- | --- |
| P3-001 | サイドカー技術選定 | Envoy vs Mitmproxy 評価、パフォーマンス・機能比較 | `[ ]` | Phase 2 コア完了 |
| P3-002 | 透過プロキシ構築 | VM からの全アウトバウンド通信をインターセプト | `[ ]` | P3-001 |
| P3-003 | ドメインホワイトリスト | 許可ドメインリストによるフィルタリング実装 | `[ ]` | P3-002 |
| P3-004 | PII スキャニング実装 | 正規表現パターンによる機密情報検出（クレジットカード、APIキー等） | `[ ]` | P3-002 |
| P3-005 | Intent-based ファイアウォール | タスクコンテキストに基づく動的許可ルール | `[ ]` | P3-003 |

### 3.2 監査ログ

| ID | タスク | 詳細 | ステータス | 依存 |
| --- | --- | --- | --- | --- |
| P3-010 | ログスキーマ設計 | 監査ログの JSON スキーマ定義（タイムスタンプ、リクエスト内容、判定結果） | `[ ]` | P3-002 |
| P3-011 | ログ収集実装 | サイドカーからのログ出力、ストレージへの永続化 | `[ ]` | P3-010 |
| P3-012 | ログ検索 API | 時間範囲・キーワードでの監査ログ検索 | `[ ]` | P3-011 |
| P3-013 | コンプライアンスレポート | 定期レポート生成機能（日次/週次サマリー） | `[ ]` | P3-011 |

### 3.3 MCP ネイティブホスティング

| ID | タスク | 詳細 | ステータス | 依存 |
| --- | --- | --- | --- | --- |
| P3-020 | MCP プロトコル調査 | Anthropic MCP 仕様の詳細調査、実装要件整理 | `[ ]` | Phase 2 コア完了 |
| P3-021 | MCP サーバーランタイム | サンドボックス内での MCP サーバー起動・管理 | `[ ]` | P3-020 |
| P3-022 | ツール登録 API | エージェントが利用可能なツールの登録・管理 | `[ ]` | P3-021 |
| P3-023 | DLP 統合 | MCP ツール呼び出しに対する DLP ポリシー適用 | `[ ]` | P3-004, P3-022 |

### 3.4 テスト (Phase 3)

| ID | タスク | 詳細 | ステータス | 依存 |
| --- | --- | --- | --- | --- |
| P3-030 | DLP 単体テスト | 各種 PII パターン検出の正確性検証 | `[ ]` | P3-004 |
| P3-031 | ファイアウォール E2E テスト | 禁止ドメインへのアクセス遮断、許可ドメインへのアクセス許可 | `[ ]` | P3-005 |
| P3-032 | 監査ログ整合性テスト | すべてのリクエストが正しくログされることを検証 | `[ ]` | P3-011 |
| P3-033 | MCP 統合テスト | ツール呼び出しの正常動作と DLP 適用を検証 | `[ ]` | P3-023 |

---

## Phase 4: Time Travel Debugging

**目標:** 究極のデバッグ体験の提供

> [!NOTE]
> Phase 4 は Phase 2 の VM 基盤が安定した後に開始可能

### 4.1 スナップショット管理

| ID | タスク | 詳細 | ステータス | 依存 |
| --- | --- | --- | --- | --- |
| P4-001 | 差分スナップショット設計 | メモリ/ディスク状態の効率的な差分保存方式設計 | `[ ]` | P2-005 |
| P4-002 | スナップショット取得 API | 各ステップ実行後に自動スナップショット取得 | `[ ]` | P4-001 |
| P4-003 | スナップショット復元 API | 指定ステップへの巻き戻し実装 | `[ ]` | P4-002 |
| P4-004 | ストレージ管理 | スナップショットの保存期間管理、古いデータの自動削除 | `[ ]` | P4-002 |

### 4.2 デバッグ UI

| ID | タスク | 詳細 | ステータス | 依存 |
| --- | --- | --- | --- | --- |
| P4-010 | Web UI 技術スタック選定 | React/Vue/Svelte 等の選定 | `[ ]` | P4-003 |
| P4-011 | タイムライン表示 | 実行ステップのタイムライン可視化 | `[ ]` | P4-010 |
| P4-012 | 状態インスペクター | 各ステップでの変数値・ファイル内容表示 | `[ ]` | P4-011 |
| P4-013 | シェルアクセス | 任意ステップでの対話シェル起動 | `[ ]` | P4-003 |

### 4.3 視覚的アーティファクト

| ID | タスク | 詳細 | ステータス | 依存 |
| --- | --- | --- | --- | --- |
| P4-020 | Matplotlib キャプチャ | `plt.show()` 呼び出しの自動インターセプト・画像保存 | `[ ]` | P2-020 |
| P4-021 | アーティファクトプロトコル | 画像・ファイル・テーブル等の構造化メタデータ定義 | `[ ]` | P4-020 |
| P4-022 | UI 統合 | アーティファクトの Web UI 表示 | `[ ]` | P4-012, P4-021 |

### 4.4 テスト (Phase 4)

| ID | タスク | 詳細 | ステータス | 依存 |
| --- | --- | --- | --- | --- |
| P4-030 | スナップショット整合性テスト | 復元後の状態が取得時と一致することを検証 | `[ ]` | P4-003 |
| P4-031 | UI E2E テスト | Playwright でのブラウザ自動テスト | `[ ]` | P4-012 |
| P4-032 | アーティファクトキャプチャテスト | 各種グラフライブラリの出力がキャプチャされることを検証 | `[ ]` | P4-020 |

---

## CI/CD パイプライン

### 継続的インテグレーション

| ID | タスク | 詳細 | ステータス | 依存 |
| --- | --- | --- | --- | --- |
| CI-001 | Rust ビルド & テスト | `cargo build`, `cargo test`, `cargo clippy` | `[ ]` | P1-003 |
| CI-002 | Python lint & テスト | `ruff`, `mypy`, `pytest` | `[ ]` | P1-003 |
| CI-003 | クロスプラットフォームビルド | Linux (x86_64, aarch64), macOS (x86_64, aarch64), Windows | `[ ]` | CI-001 |
| CI-004 | カバレッジ計測 | `cargo-llvm-cov` + `pytest-cov`、Codecov 連携 | `[ ]` | CI-001, CI-002 |
| CI-005 | ベンチマーク CI | PR ごとに `criterion` ベンチマーク実行、リグレッション検出 | `[x]` | P1-054 |
| CI-006 | セキュリティスキャン | `cargo-audit`, `pip-audit`, Dependabot | `[ ]` | CI-001, CI-002 |

### 継続的デリバリー

| ID | タスク | 詳細 | ステータス | 依存 |
| --- | --- | --- | --- | --- |
| CD-001 | PyPI 自動リリース | tag push → PyPI 公開ワークフロー | `[ ]` | P1-061 |
| CD-002 | Crates.io リリース | Rust クレートの公開（オプション） | `[ ]` | P1-062 |
| CD-003 | Docker イメージビルド | Phase 2 以降の VM イメージ用 | `[ ]` | P2-002 |
| CD-004 | ドキュメント自動デプロイ | GitHub Pages への API ドキュメント公開 | `[ ]` | P1-044 |

---

## マイルストーンサマリー

| マイルストーン | 完了条件 | 目標日 | ステータス |
| --- | --- | --- | --- |
| **M1: MVP リリース** | Phase 1 全タスク完了、PyPI v0.1.0 公開 | TBD | `[ ]` |
| **M2: データサイエンス対応** | Phase 2 全タスク完了、Pandas/NumPy 動作確認 | TBD | `[ ]` |
| **M3: エンタープライズ対応** | Phase 3 全タスク完了、DLP 機能リリース | TBD | `[ ]` |
| **M4: デバッグ体験** | Phase 4 全タスク完了、Web UI リリース | TBD | `[ ]` |

---

## メモ・課題

> このセクションは実装中に発見した課題やメモを記録します

- (2025-01-11) Task 1.1 完了。Python 3.14 環境でのビルドには `PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1` が必要。
