# EnterSandBox Test Bench Plan (Phase 1 / Tier 1)

## 1. 目的

`Sandbox` の「実際の使い方」に近い利用シナリオを、単発の API テストではなく
シナリオ実行型のテストベンチとして定義し、以下を継続的に検証できる状態を作る。

1. 主要ユースケースでの機能正当性
2. 制限系（timeout / output limit / import policy）の期待どおりの失敗
3. 回帰検知に使える実行メトリクス（時間・メモリ・失敗率）
4. PR と定期実行での運用可能性

本ドキュメントは **実装計画** を定義する。コード実装は本タスク範囲外。

## 2. 背景と現状

- `tests/python/test_sandbox.py` で SDK の E2E ケースは整備済み（正常系/異常系）
- `scripts/check_tier1_benchmarks.py` で性能回帰ガードは導入済み
- ただし現状は「機能の個別検証」と「性能ガード」が分離しており、実利用に近い
  複数ステップのジャーニーを通した評価レイヤーが未定義

## 3. スコープ

### 3.1 対象（In Scope）

- Phase 1 の Tier 1 実装（`Sandbox`, `SandboxConfig`, `SandboxResult`）
- Python 側からの実行シナリオ（`agentbox` 公開 API）
- ローカル実行と CI 実行の両方で再現可能なベンチ運用

### 3.2 非対象（Out of Scope）

- Tier 2（Firecracker）実行検証
- Network DLP / Time Travel 機能の実装検証
- ベンチ実行結果に基づく自動チューニング機能

## 4. テストベンチ方針

### 4.1 レイヤー分離

既存テスト群は維持し、以下の役割分離を採用する。

- 既存 `pytest`:
  API 契約・エッジケースの厳密検証（速い失敗検出）
- 新規テストベンチ:
  実利用シナリオ単位での通し検証 + メトリクス記録
- 既存ベンチガード:
  cold start / warm peak の閾値判定

### 4.2 設計原則

1. 再現性優先: 入力コード・期待値・閾値は固定化し、乱数は seed 固定
2. 可観測性優先: 実行ごとに JSON 結果を保存し、失敗理由を構造化
3. 運用分離: `quick`（PR）と `full`（定期）を明確に分ける
4. 既存資産活用: `scripts/check_tier1_benchmarks.py` を perf モードで再利用

## 5. シナリオ仕様（v1）

### 5.1 共通ルール

- 各シナリオは `given / when / then` を持つ
- 各シナリオは `success criteria` と `failure signature` を持つ
- 各シナリオで `duration_ms` を計測
- 同一 run 内での Sandbox インスタンス再利用有無を明示

### 5.2 シナリオ一覧

| ID | 名称 | 目的 | 主な検証点 |
| --- | --- | --- | --- |
| TB-01 | Basic Run | 最小実行の成立確認 | stdout/stderr/exit_code |
| TB-02 | Multi-turn VFS | 実利用の複数run操作 | run 間の VFS 永続 |
| TB-03 | Import Guardrail | ガードレール確認 | allowed/disallowed import |
| TB-04 | Resource Limits | 制限系失敗の妥当性 | timeout, max_output_bytes |
| TB-05 | Error Semantics | エラー解釈の一貫性 | Python 例外、sys.exit |
| TB-06 | Lightweight Workflow | 軽量実務フロー模擬 | json/re/datetime を跨ぐ処理 |

### 5.3 各シナリオ契約

#### TB-01 Basic Run

- Given: デフォルト設定の `Sandbox`
- When: `print('hello')`
- Then:
  - `exit_code == 0`
  - `stdout.strip() == "hello"`
  - `stderr == ""`

#### TB-02 Multi-turn VFS

- Given: 同一 `Sandbox` インスタンス
- When:
  1. run1 でファイル作成
  2. run2 で同ファイル読取
- Then:
  - run2 で run1 の書き込み内容を読める
  - すべて `exit_code == 0`

#### TB-03 Import Guardrail

- Given: `SandboxConfig(allowed_modules=["json", "collections"])`
- When:
  1. 許可 import（成功ケース）
  2. 未許可 import（失敗ケース）
- Then:
  - 成功ケースは `exit_code == 0`
  - 失敗ケースは `RuntimeError` で
    `Import blocked by SandboxConfig.allowed_modules` を含む

#### TB-04 Resource Limits

- Given:
  - timeout ケース: `timeout_ms=20`
  - output ケース: `max_output_bytes=256` など小さい値
- When:
  - 無限ループまたは長時間処理
  - 過大出力
- Then:
  - timeout は `Execution timed out after <N> ms`
  - output は `max_output_bytes=<N>` を含むエラー

#### TB-05 Error Semantics

- Given: デフォルト設定の `Sandbox`
- When:
  1. `raise RuntimeError(...)`
  2. `sys.exit(123)`
- Then:
  - (1) では Python 例外情報が stderr/失敗結果に反映
  - (2) では `exit_code == 123`

#### TB-06 Lightweight Workflow

- Given: 同一 `Sandbox` インスタンス
- When:
  1. JSON を読み込み
  2. 正規表現抽出
  3. 日付整形
  4. 結果を集約して出力
- Then:
  - 期待フォーマットの集約結果が得られる
  - 実行失敗率 0%（所定回数内）

## 6. 実行モード設計

| モード | 用途 | 対象シナリオ | 反復回数 | 想定実行場所 |
| --- | --- | --- | --- | --- |
| quick | PR の高速回帰検知 | TB-01, TB-03, TB-04 | 各1回 | CI / 開発者ローカル |
| full | 通し品質確認 | TB-01..TB-06 | 各3回以上 | nightly CI / リリース前 |
| perf | 性能監視統合 | TB-01 + 既存ベンチガード | シナリオ1回 + bench | PR / nightly |

補足:
- `perf` は `scripts/check_tier1_benchmarks.py` の結果を統合表示する
- `full` は flakiness 検出のため最低 3 回反復を基本値とする

## 7. 計測・レポート仕様

### 7.1 収集メトリクス

- `duration_ms`（シナリオ単位、run 単位）
- `pass/fail`（シナリオ単位）
- `failure_signature`（正規化した失敗識別子）
- `process_peak_rss_kb`（取得可能環境のみ）
- `success_rate`（反復実行時）

### 7.2 結果フォーマット（JSON）

結果は機械読取可能な JSON を標準とする。

```json
{
  "version": "tb-v1",
  "mode": "quick",
  "timestamp_utc": "2026-03-06T12:00:00Z",
  "environment": {
    "python": "3.11.x",
    "platform": "linux-x86_64"
  },
  "scenarios": [
    {
      "id": "TB-01",
      "status": "pass",
      "iterations": 1,
      "duration_ms": {"p50": 12.3, "p95": 12.3}
    }
  ],
  "summary": {
    "total": 3,
    "passed": 3,
    "failed": 0
  }
}
```

### 7.3 失敗時出力

- 失敗シナリオ ID
- 失敗したステップ番号
- 期待値と実測値
- 原文エラー抜粋（長すぎるログはトリム）

## 8. CI 統合方針

### 8.1 PR（軽量）

- `quick` を実行
- `perf` を必要に応じて実行（既存 PR ベンチガードと整合）
- 失敗時はジョブを fail

### 8.2 定期実行（重め）

- `full` を nightly 実行
- 実行結果 JSON を artifacts として保存
- 連続失敗時は flaky でなく regression と判断できるよう履歴比較可能にする

### 8.3 ローカル運用

- 同一コマンド体系で `quick/full/perf` を実行可能にする
- CI 専用パラメータは環境変数で上書き可能にする

## 9. 実装タスク分解（計画）

### 9.1 タスク一覧

| ID | タスク | 主要成果物 | ステータス | 依存 |
| --- | --- | --- | --- | --- |
| TB-001 | ベンチ仕様固定 | `docs/TB_PLAN.md` v1 | `[x]` | - |
| TB-002 | シナリオ定義のデータ契約設計 | シナリオ schema（Python dict/JSON） | `[x]` | TB-001 |
| TB-003 | テストベンチランナー骨格 | 実行 CLI（mode 指定） | `[x]` | TB-002 |
| TB-004 | TB-01〜TB-03 実装 | quick 対象シナリオ実装 | `[x]` | TB-003 |
| TB-005 | TB-04〜TB-06 実装 | full 対象シナリオ実装 | `[x]` | TB-004 |
| TB-006 | メトリクス集計・JSON 出力 | 結果レポート実装 | `[x]` | TB-003 |
| TB-007 | perf モード連携 | 既存ベンチガード統合表示 | `[x]` | TB-006 |
| TB-008 | CI 連携（quick/full） | workflow 更新 | `[x]` | TB-005, TB-006 |
| TB-009 | 運用ドキュメント | 実行手順・失敗時対応手順 | `[x]` | TB-008 |
| TB-010 | 安定化 | flaky 解析と閾値チューニング | `[ ]` | TB-008 |

### 9.2 実装順序（推奨）

1. TB-002, TB-003 で枠組みを先に固定
2. TB-004 を先行実装して `quick` を早期運用
3. TB-005, TB-006 で `full` の品質を完成
4. TB-007, TB-008 で CI 統合
5. TB-009, TB-010 で運用を安定化

## 10. 受け入れ基準

### 10.1 機能面

1. `quick` が PR 上で安定完走する
2. `full` がローカルと CI で同一判定を返す
3. 全シナリオで失敗時の原因特定に必要な情報が JSON に入る

### 10.2 運用面

1. 新規シナリオの追加が定義ファイル編集のみで可能
2. CI の実行時間増加が許容範囲に収まる
3. 性能回帰判定は既存 `check_tier1_benchmarks.py` と矛盾しない

## 11. リスクと対策

| リスク | 影響 | 対策 |
| --- | --- | --- |
| 実行環境差で時間が揺れる | 偽陽性 fail | quick は機能中心、perf は閾値緩衝と履歴比較 |
| 既存 pytest と役割重複 | 保守コスト増 | シナリオは「複数ステップ実利用」に限定 |
| ログ過多で読みにくい | 調査効率低下 | エラー抜粋 + JSON 構造化 + 最大長制限 |
| CI 時間増加 | 開発速度低下 | PR は quick のみ、full は nightly 中心 |

## 12. 更新ルール

- TB の仕様変更時は本ドキュメントを先に更新する
- 変更時はタスク ID と依存を更新し、`docs/PLAN.md` のメモにも要点を追記する
- 日付付きの運用判断（閾値変更など）は履歴を残す

## 13. 実施メモ

- (2026-03-06) TB-002〜TB-009 を実装。`scripts/run_tier1_testbench.py` を追加し、
  `TB-01..TB-06` シナリオ、`quick/full/perf` モード、JSON 出力、
  `scripts/check_tier1_benchmarks.py` 連携を実装。
- (2026-03-06) `.github/workflows/testbench.yml` を追加し、PR で `quick`、nightly で
  `full` + `perf` を実行する CI 導線を追加。成果 JSON を artifact 化。
- (2026-03-06) `README.md` / `README.ja.md` にテストベンチ実行手順を追加。

---

初版作成日: 2026-03-06
対象コードベース時点: `main`（Phase 1 完了、Phase 2 一部着手）
