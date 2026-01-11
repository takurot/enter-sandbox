# ⏳ EnterSandBox

**Governance-First AI Agent Runtime Platform**

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Status: Planning](https://img.shields.io/badge/Status-Planning-yellow)](docs/PLAN.md)

EnterSandBoxは、自律型AIエージェントの「ガバナンス」と「可観測性」に焦点を当てた、次世代のコード実行プラットフォームです。
単なる高速なコード実行環境（Runner）にとどまらず、ハイブリッドランタイム技術を用いることで「速度」と「互換性」の両立を実現し、同時に企業のセキュリティ要件を満たす強力な統制機能を提供します。

---

## 🚀 Why EnterSandBox?

自律型AIエージェントの台頭により、インフラストラクチャには新たな要件が求められています。

*   **信頼できないコードの実行:** エージェントが生成するコードは未知であり、潜在的に危険です。
*   **レイテンシへの感受性:** チャットUXを損なわないミリ秒単位の起動速度が必要です。
*   **データサイエンスの壁:** 軽量なWASMだけでは、PandasやNumPyといった必須ライブラリが動きません。
*   **ガバナンスの欠如:** 従来のアプローチでは、エージェントによる情報漏洩（DLP）や意図しないアクセスを防げません。

EnterSandBoxは、これらの課題を解決する**「AIエージェントのためのOS」**です。

## ✨ Key Features

### 1. Hybrid Runtime Architecture
タスクの性質に応じて、最適なランタイムを動的に選択・ルーティングします。

| Tier | 名称 | 技術 | 起動速度 | 用途 |
| --- | --- | --- | --- | --- |
| **Tier 1** | **Nano-Sandbox** | Wasmtime + RustPython | **< 10ms** | 制御ロジック、文字列操作、JSONパース |
| **Tier 2** | **Heavy-Sandbox** | Firecracker MicroVM | **~150ms** | データ分析(Pandas), 機械学習, 複雑な依存関係 |

### 2. Agency Governance (Network DLP)
エージェントの暴走を防ぎ、企業コンプライアンスを遵守します。

*   **PII Scanning:** 通信内容をリアルタイム検査し、APIキーや個人情報の流出を遮断。
*   **Intent-based Whitelist:** エージェントの「現在の意図」に基づいて、アクセス可能なドメインを動的に制限。
*   **監査ログ:** 全てのアクションと通信を記録し、完全なトレーサビリティを提供。

### 3. Time Travel Debugging
開発者体験（DX）を革新するデバッグ機能を提供します。

*   **Stepwise Snapshots:** 実行の各ステップでメモリとディスクの状態を保存。
*   **Rewind & Inspect:** エラー発生直前の状態に「巻き戻し」、変数の値やファイルの中身を調査可能。

## 🛠 Architecture

```mermaid
graph TD
    UserCode[User Code / Agent Action] --> Router[Adaptive Runtime Router]
    
    Router -->|Logic / Text Processing| Tier1[Tier 1: Nano-Sandbox (Wasm)]
    Router -->|Data Science / Heavy Compute| Tier2[Tier 2: Heavy-Sandbox (MicroVM)]
    
    subgraph Governance
        Sidecar[Network DLP Sidecar]
    end
    
    Tier1 -.-> Sidecar
    Tier2 -.-> Sidecar
    Sidecar --> Internet((Internet))
```

## 🧩 Usage (Preview)

ユーザーは背後のランタイムを意識することなく、統一されたAPIを利用できます。

```python
from agentbox import Sandbox

# 自動ルーティングモード
box = Sandbox()

code = """
import pandas as pd
# 自動的に Tier 2 (MicroVM) が選択されます
df = pd.DataFrame({"A": [1, 2, 3]})
print(df.describe())
"""

result = box.run(code)
print(result.stdout)
```

## 🗺 Roadmap

詳細は [docs/PLAN.md](docs/PLAN.md) を参照してください。

- **Phase 1:** Nano-Sandbox (MVP) - Wasmベースの超高速実行環境
- **Phase 2:** Heavy-Sandbox & Routing - Firecracker統合とデータサイエンス対応
- **Phase 3:** Governance & Security - ネットワークDLPとMCPネイティブ対応
- **Phase 4:** Time Travel - デバッグ機能とUIの実装

## 📚 Documentation

- [機能仕様書 (SPEC.md)](docs/SPEC.md)
- [実装計画 (PLAN.md)](docs/PLAN.md)
- [リサーチレポート (RESEARCH.md)](docs/RESEARCH.md)

## 🤝 Contributing

EnterSandBoxはオープンソースプロジェクトとして開発される予定です。
貢献ガイドラインは準備中です。

## 📄 License

MIT License (Planned)
