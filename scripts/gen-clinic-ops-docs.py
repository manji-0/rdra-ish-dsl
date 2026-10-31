#!/usr/bin/env python3
"""Generate samples/clinic-ops/design-doc.md (engineers) and samples/clinic-ops/prd.md (PdM).

Every fact comes from the model through the CLI: `list ... --format json` for the elements and
their links, `list` and `csv` with `--format markdown` for the tables, and the diagrams that
scripts/sample-artifacts.tsv commits under samples/clinic-ops/out/. The script holds only the
chapter order and the sentences that introduce each chapter.

scripts/check-sample-artifacts.sh runs it after regenerating the artifacts and fails when a
document differs from what is committed:

    python3 -I scripts/gen-clinic-ops-docs.py target/debug/rdra-ish
"""
import json
import re
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SAMPLE = ROOT / 'samples/clinic-ops'
OUT = SAMPLE / 'out'
BIN = Path(sys.argv[1] if len(sys.argv) > 1 else 'target/debug/rdra-ish').resolve()
MODEL = 'samples/clinic-ops'

# The BUC chapters in the order a visit runs through the hospital.
CHAPTERS = [
    'BucPatientOnboarding', 'BucAppointmentScheduling', 'BucVisitCheckIn', 'BucClinicalEncounter',
    'BucOrdersResults', 'BucPrescriptionFulfillment', 'BucVisitPayment', 'BucBillingClaims',
    'BucFollowupCare', 'BucStaffAdministration',
]
# The lifecycles whose reachable states the design document also lists, by committed table.
STATE_TABLES = {'Appointment': 'states_appointment.txt', 'Prescription': 'states_prescription.txt',
                'ClaimStatement': 'states_claim_statement.txt'}


def cli(*args):
    return subprocess.run([BIN, *args], cwd=ROOT, check=True, capture_output=True, text=True).stdout


def listed(kind):
    return json.loads(cli('list', MODEL, '--kind', kind, '--format', 'json'))


def md_list(kind):
    return cli('list', MODEL, '--kind', kind, '--format', 'markdown')


def md_csv(kind):
    with tempfile.TemporaryDirectory() as tmp:
        out = Path(tmp) / 'table.md'
        cli('csv', MODEL, '--kind', kind, '--format', 'markdown', '--out', str(out))
        return out.read_text()


def ids(cell):
    return [x for x in cell.split('|') if x]


def cell(text):
    return text.replace('|', '\\|').replace('\n', ' ')


def code(xs):
    return ', '.join(f'`{x}`' for x in xs) if xs else '（なし）'


def mermaid(rel):
    return '```mermaid\n' + (OUT / rel).read_text().rstrip('\n') + '\n```\n'


def folded(summary, body):
    return f'<details><summary>{summary}</summary>\n\n{body.rstrip()}\n\n</details>\n'


goals = listed('goal')
requirements = listed('requirement')
actors = listed('actor')
bucs = {b['id']: b for b in listed('buc')}
usecases = {u['id']: u for u in listed('usecase')}
rules = listed('rule')
goal_label = {g['id']: g['label'] for g in goals}
entity_label = {row['entity_id']: row['entity_label'] for row in listed('entity')}

assert set(CHAPTERS) == set(bucs), set(bucs) ^ set(CHAPTERS)
in_chapters = {uc for b in CHAPTERS for uc in ids(bucs[b]['usecases'])}
assert in_chapters == set(usecases), set(usecases) ^ in_chapters
undescribed = [uc for uc, u in usecases.items() if not u['description']]
assert not undescribed, f'use cases without a description: {undescribed}'
assert all(bucs[b]['description'] for b in CHAPTERS)


def guards(uc):
    """The labels of the conditions that guard `uc`, directly or through an API it invokes."""
    targets = {uc, *ids(usecases[uc]['apis'])}
    return [r['label'] for r in rules if r['kind'] == 'condition' and targets & set(ids(r['applies_to']))]


def stem(buc):
    """`BucVisitCheckIn` -> `visit_check_in`, the stem of the BUC's files in out/buc/."""
    return ''.join('_' + c.lower() if c.isupper() else c for c in buc[3:]).lstrip('_')


HEAD = '''<!-- derived-from ../../docs/language-reference.md -->
<!-- derived-from ../../docs/cli-reference.md -->

`scripts/gen-clinic-ops-docs.py` が `samples/clinic-ops/` のモデルから CLI を通して生成する。図・表・説明文を手で直さず、`.rdra` を直して `scripts/check-sample-artifacts.sh` を実行する。題材は日本の病院の外来業務（受付、資格確認、診察、検査、処方、計算・会計、レセプト請求）で、入院は対象外。業務規則は `.rdra` の `//` コメントに一次資料の出典（ファイル名:行）を付けている。
'''

# ── design-doc.md ──────────────────────────────────────────────────────────────

d = ['# Clinic Ops 設計書\n\n', HEAD, '''
この文書は実装するエンジニア向けである。構造、処理、仕様の順に読む。

1. BUC ごとのロバストネス図で、画面・制御・エンティティの構造をつかむ。
2. UC ごとのシーケンス図で、ガードと書き込みの順序をつかむ。
3. 状態モデル、データモデル、規則、CRUD と権限で仕様を確かめる。

要求と成功指標は [PRD](prd.md) にある。

## 1. 概要

対象は病院の外来業務である。業務を 10 の BUC に分け、内部システムは `ClinicOpsSystem` 1 つにまとめている。院外の仕組み（オンライン資格確認、精算機、外注検査、電子処方箋、保険薬局、審査支払機関、患者への通知）は外部システムとしてイベントでつなぐ。

米国型の診療所モデルとの主な違いは次のとおりである。

| 論点 | このモデル |
|---|---|
| 支払の時点 | 受付では支払わない。診察の後に計算受付・会計受付で精算する |
| 窓口負担 | 負担割合（3割・2割）で計算し、限度額情報があれば自己負担限度額までを窓口で受け取る |
| 資格確認 | 受付のたびにオンライン資格確認等システムで確認する。来院前の一括照会、申立書、システム障害時モードがある |
| 請求 | レセプトを月ごとに審査支払機関（支払基金・国保連）へ請求する。返戻は再請求でき、査定は再審査請求で争う |
| 処方 | 院外処方箋は電子処方箋管理サービスへ登録し、患者が選ぶ保険薬局で調剤する。院内処方は薬剤師が調剤・監査する |
| 紹介 | 紹介状なしの初診は選定療養費（自費）の対象。救急搬送などは除外 |

### システムコンテキスト図

システムを使う actor と、イベントでつながる外部システムを示す。actor からシステムへの線は、actor が担う BUC の UC が呼ぶ API から導出される。

''', mermaid('context.mmd'), '''
## 2. BUC ごとの構造と処理

各節は BUC の説明、ロバストネス図、画面遷移図、UC の表、UC ごとのシーケンス図の順に並ぶ。ロバストネス図では actor が画面を使い、画面が制御（UC が呼ぶ API、API がなければ UC 自身）を呼び、制御がエンティティを読み書きする。画面遷移図（`diagram --kind screen --buc <Buc>`）の実線は `navigates` で宣言した遷移で、ラベルは遷移を起こす UC である。点線の「業務上の順番」は宣言がない所を業務フローから補ったもので、同じ担当者のレーンで続く手順が表示する画面の順を示し、実際の画面遷移ではない。遷移も業務上の順番もない BUC では画面遷移図を省く。表の「ガード」は UC に効く条件で、式は [5. 規則](#5-規則) にある。

''']
for n, buc in enumerate(CHAPTERS, 1):
    b = bucs[buc]
    d.append(f"### 2.{n} {b['label']}\n\n{b['description']}\n\n")
    d.append(mermaid(f'buc/robustness_{stem(buc)}.mmd') + '\n')
    screen = f'buc/screen_{stem(buc)}.mmd'
    # An edge line, labelled (`-->|...|`) or not (`--> `).
    if re.search(r' (-->|-\.->)[| ]', (OUT / screen).read_text()):
        d.append('#### 画面遷移図\n\n' + mermaid(screen) + '\n')
    d.append('| UC | 説明 | 画面 | 制御 | ガード |\n|---|---|---|---|---|\n')
    for uc in ids(b['usecases']):
        u = usecases[uc]
        control = ids(u['apis']) or [uc]
        d.append(f"| {cell(u['label'])}<br>`{uc}` | {cell(u['description'])} | {code(ids(u['screens']))} "
                 f"| {code(control)} | {cell('、'.join(guards(uc))) or '（なし）'} |\n")
    d.append('\n')
    for uc in ids(b['usecases']):
        d.append(folded(f"{usecases[uc]['label']}（{uc}）のシーケンス図", mermaid(f'uc/sequence_{uc}.mmd')) + '\n')

d.append('''## 3. 状態モデル

状態モデルを持つエンティティごとに、状態図（`diagram --kind state --entity <Entity>`）を示す。遷移のラベルは状態を動かす UC のラベルで、`[...]` はその遷移を守る条件である。主なライフサイクルには、到達可能な状態の表（`states --format table --entity <Entity>`）を付ける。表の `INITIAL` は作成直後、`TERMINAL` は抜け出さない状態、`VIA` はその状態に至る UC である。

''')
for path in sorted((OUT / 'state').glob('*.mmd')):
    entity = path.stem
    d.append(f'### {entity_label[entity]}（{entity}）\n\n' + mermaid(path.relative_to(OUT)) + '\n')
    if entity in STATE_TABLES:
        table = (OUT / STATE_TABLES[entity]).read_text().rstrip('\n')
        d.append(folded('到達可能な状態の表', '```text\n' + table + '\n```') + '\n')
assert set(STATE_TABLES) <= {p.stem for p in (OUT / 'state').glob('*.mmd')}

d.append('''## 4. データモデル

ER 図はエンティティと外部キーを示す。列の一覧は `list --kind entity --format markdown` の出力である。

''')
d.append(mermaid('er.mmd') + '\n')
d.append(folded('エンティティの列の一覧', md_list('entity')) + '\n')

d.append('''## 5. 規則

`list --kind rule --format markdown` の出力である。`condition` は UC を守る条件、それ以外は状態検証（`rdra-ish check` と `verify`）が確かめる制約である。値はラベルで書いている。

''')
d.append(md_list('rule') + '\n')

d.append('''## 6. CRUD と権限

### API とエンティティの CRUD

`csv --kind api-matrix --format markdown` の出力である。1 つの API が多くのエンティティを書き換えていないか、参照の API と書き込みの API が混ざっていないかを見る。

''')
d.append(md_csv('api-matrix') + '\n')
d.append('''### actor の権限割当

`list --kind actor-permission-audit --format markdown` の出力である。`missing` は actor が実行する UC/API に必要な権限が付いていないこと、`excess` は付いているが実行経路で使われない権限を示す。

''')
d.append(md_list('actor-permission-audit') + '\n')

d.append('## 7. 非機能要件と制約\n\n### 非機能要件\n\n')
d.append(md_list('nfr') + '\n### 制約\n\n' + md_list('constraint') + '\n')

d.append('''## 8. イベント連携

UC と外部システムが出すイベントと、それが起動する UC・BUC・外部システム、タイマーが起動する UC を示す。イベントの契約は AsyncAPI として [`out/asyncapi.json`](out/asyncapi.json) に出力している（`export --kind asyncapi`）。HTTP の契約（`path`・`method`）を持つ API がないので、OpenAPI は出力していない。

''')
d.append(mermaid('event_flow.mmd') + '\n')

d.append('''## 9. 付録

RDRA の層ごとの図である。設計の読み順には入れていない。

- BUC 別の UC複合図: ''' + ', '.join(f"[{bucs[b]['label']}](out/buc/usecase_{stem(b)}.mmd)" for b in CHAPTERS) + '''
- 診察 BUC の RDRA 全体図: [`out/rdra_buc_clinical_encounter.mmd`](out/rdra_buc_clinical_encounter.mmd)

''')
d.append(folded('業務の流れ図（ビジネスコンテキスト）', mermaid('business.mmd')))
d.append('\n' + folded('全画面の画面遷移図', mermaid('screen.mmd')))

# ── prd.md ─────────────────────────────────────────────────────────────────────

in_scope = [r for r in requirements if r['status'] != 'out-of-scope']
out_of_scope = [r for r in requirements if r['status'] == 'out-of-scope']

p = ['# Clinic Ops PRD\n\n', HEAD, '''
この文書はプロダクトマネージャー向けである。なぜ作るか、誰のためか、何を満たせばよいかを示す。画面・API・データの設計は [設計書](design-doc.md) にある。

## 1. 背景と目的

成功指標の現状値と目標値はモデルが置いた想定の数字で、一次資料はない。実際の病院で使うときは、その病院の実績で置き換える。

''']
for g in goals:
    p.append(f"### {g['label']}\n\n{g['description']}\n\n")
    p.append('| 指標 | 現状 | 目標 | 期間 |\n|---|---|---|---|\n')
    p.append(f"| {cell(g['metric'])} | {cell(g['baseline'])} | {cell(g['target'])} | {cell(g['window'])} |\n\n")

p.append('## 2. 対象ユーザー\n\n| 利用者 | 役割 |\n|---|---|\n')
for a in actors:
    p.append(f"| {cell(a['label'])} | {cell(a['description'])} |\n")
p.append('''
### 誰がどの業務に関わるか

`list --kind buc-roles --format markdown` の出力である。`担当` は利用者がその BUC の UC を行うこと、`イベント送信` と `イベント受信` は外部システムがその BUC とイベントをやり取りすることを示す。

''' + md_list('buc-roles') + '\n### システムコンテキスト図\n\n' + mermaid('context.mmd') + '\n')

p.append('''## 3. 要求

要求を目的ごとにまとめる。「BUC」はその要求が動機付ける業務で、[5. ユーザージャーニー](#5-ユーザージャーニー) にその業務フローがある。次の図は目的、要求、BUC のつながりである。「やらないこと」の枠にある要求は [4. やらないこと](#4-やらないこと) に載せている。

''' + mermaid('requirement.mmd') + '\n')
for g in goals:
    p.append(f"### {g['label']}\n\n")
    p.append('| ID | 要求 | 優先度 | 関係者 | 目的 | 受け入れ条件 | 出典 | BUC |\n|---|---|---|---|---|---|---|---|\n')
    for r in in_scope:
        if g['id'] not in ids(r['goals']):
            continue
        p.append(f"| `{r['id']}` | {cell(r['label'])} | {r['priority']} | {cell('、'.join(ids(r['stakeholders'])))} "
                 f"| {cell('、'.join(goal_label[x] for x in ids(r['goals'])))} "
                 f"| {cell(' '.join(ids(r['acceptance_criteria'])))} | {cell('; '.join(ids(r['sources'])))} "
                 f"| {cell('、'.join(bucs[x]['label'] for x in ids(r['bucs']))) or '（なし）'} |\n")
    p.append('\n')
unplaced = [r['id'] for r in in_scope if not ids(r['goals'])]
assert not unplaced, f'requirements with no goal: {unplaced}'

p.append('## 4. やらないこと\n\n| ID | 内容 | 理由 |\n|---|---|---|\n')
for r in out_of_scope:
    p.append(f"| `{r['id']}` | {cell(r['label'])} | {cell(r['rationale'])} |\n")

p.append('''
## 5. ユーザージャーニー

次の図は業務の流れである。業務ごとに BUC をまとめ、ある BUC から次の BUC への引き渡しを、引き渡すイベントのラベルで示す。

''' + mermaid('business.mmd') + '''
この後の節は BUC ごとの業務フローである。手順、分岐、例外と、各手順を担う UC を示す。業務フローを持たない BUC（病院管理）は載せていない。

''')
for buc in CHAPTERS:
    flow = OUT / f'buc/flow_{stem(buc)}.mmd'
    if flow.exists():
        p.append(f"### {bucs[buc]['label']}\n\n" + mermaid(flow.relative_to(OUT)) + '\n')

p.append('''## 6. ビジネスルール（受け入れ条件）

`list --kind rule --format markdown` の出力である。各行が、システムが守る業務規則で、受け入れ試験の条件になる。

''')
p.append(md_list('rule') + '\n')
p.append('## 7. 非機能要件\n\n' + md_list('nfr') + '\n' + md_list('constraint'))


def write(name, parts):
    text = ''.join(parts)
    (SAMPLE / name).write_text('\n'.join(line.rstrip() for line in text.rstrip('\n').split('\n')) + '\n')


write('design-doc.md', d)
write('prd.md', p)
