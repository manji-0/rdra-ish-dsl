#!/usr/bin/env python3
"""Generate design-doc.md (engineers) and prd.md (PdM) for a sample: clinic-ops (the default),
patent-ops, or food-permit-ops.

Every fact comes from the model through the CLI: `list ... --format json` for the elements and
their links, `list` and `csv` with `--format markdown` for the tables, and the diagrams that
scripts/sample-artifacts.tsv commits under samples/<sample>/out/. The script holds only, per
sample in SAMPLES, the chapter order and the sentences that introduce each chapter.

scripts/check-sample-artifacts.sh runs it for each sample after regenerating the artifacts and
fails when a document differs from what is committed:

    python3 -I scripts/gen-clinic-ops-docs.py target/debug/rdra-ish [clinic-ops|patent-ops|food-permit-ops]
"""
import json
import re
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BIN = Path(sys.argv[1] if len(sys.argv) > 1 else 'target/debug/rdra-ish').resolve()
NAME = sys.argv[2] if len(sys.argv) > 2 else 'clinic-ops'

# Per sample: the chapter order (`chapters`), the lifecycles whose reachable states the design
# document also lists by committed table (`state_tables`), and the sentences that are about the
# domain rather than the model.
SAMPLES = {
    'clinic-ops': {
        'title': 'Clinic Ops',
        # The BUC chapters in the order a visit runs through the hospital.
        'chapters': [
            'BucPatientOnboarding', 'BucAppointmentScheduling', 'BucVisitCheckIn', 'BucClinicalEncounter',
            'BucOrdersResults', 'BucPrescriptionFulfillment', 'BucVisitPayment', 'BucBillingClaims',
            'BucFollowupCare', 'BucStaffAdministration',
        ],
        'state_tables': {'Appointment': 'states_appointment.txt', 'Prescription': 'states_prescription.txt',
                         'ClaimStatement': 'states_claim_statement.txt'},
        'subject': '題材は日本の病院の外来業務（受付、資格確認、診察、検査、処方、計算・会計、レセプト請求）で、入院は対象外。',
        'overview': '''対象は病院の外来業務である。業務を 10 の BUC に分け、内部システムは `ClinicOpsSystem` 1 つにまとめている。院外の仕組み（オンライン資格確認、精算機、外注検査、電子処方箋、保険薬局、審査支払機関、患者への通知）は外部システムとしてイベントでつなぐ。

米国型の診療所モデルとの主な違いは次のとおりである。

| 論点 | このモデル |
|---|---|
| 支払の時点 | 受付では支払わない。診察の後に計算受付・会計受付で精算する |
| 窓口負担 | 負担割合（3割・2割）で計算し、限度額情報があれば自己負担限度額までを窓口で受け取る |
| 資格確認 | 受付のたびにオンライン資格確認等システムで確認する。来院前の一括照会、申立書、システム障害時モードがある |
| 請求 | レセプトを月ごとに審査支払機関（支払基金・国保連）へ請求する。返戻は再請求でき、査定は再審査請求で争う |
| 処方 | 院外処方箋は電子処方箋管理サービスへ登録し、患者が選ぶ保険薬局で調剤する。院内処方は薬剤師が調剤・監査する |
| 紹介 | 紹介状なしの初診は選定療養費（自費）の対象。救急搬送などは除外 |
''',
        'rdra_appendix': ('診察 BUC の RDRA 全体図', 'rdra_buc_clinical_encounter.mmd'),
        'metrics_note': '実際の病院で使うときは、その病院の実績で置き換える。',
        'no_flow_note': '業務フローを持たない BUC（病院管理）は載せていない。',
    },
    'patent-ops': {
        'title': 'Patent Ops',
        # The BUC chapters in the order an invention runs from the disclosure to the lapse of the right.
        'chapters': [
            'BucInventionDisclosure', 'BucFilingDecision', 'BucApplicationDrafting', 'BucFilingFormality',
            'BucExaminationRequest', 'BucOfficeAction', 'BucRegistration', 'BucAnnuity', 'BucInventorReward',
        ],
        'state_tables': {'PatentApplication': 'states_patent_application.txt',
                         'OfficeAction': 'states_office_action.txt', 'Patent': 'states_patent.txt'},
        'subject': '題材は日本の企業の特許出願業務（発明届出、出願可否の判断、特許事務所への依頼、出願と方式審査、審査請求、拒絶理由通知への応答、特許料と年金、相当の利益）で、外国出願・審判の中身・意匠と商標は対象外。',
        'overview': '''対象は企業の知的財産部が社外の特許事務所と進める特許出願業務である。業務を 9 の BUC に分け、内部システムは `IpManagementSystem`（知財管理システム）1 つにまとめている。特許事務所の弁理士はアクターとして業務フローに現れ、特許庁は外部システムとしてイベントでつなぐ。

業務の骨格は法定の期限である。期限は状態ではなくタイマー（期限管理）で表し、期限が過ぎたときに法が起こすこと（取下げ擬制・却下・消滅）を状態で表す。

| 論点 | このモデル |
|---|---|
| 権利の帰属 | 職務発明は職務発明規程により特許を受ける権利が発生時から会社に帰属する。職務発明でない発明は譲渡を受ける |
| 出願前 | 先行技術調査の後、発明審査委員会が出願・ノウハウとして秘匿・不出願を決める。出願書類は発明者の確認を経て承認する |
| 方式 | 手続補正指令には発送日から2月以内に補正する。しなければ却下される |
| 審査請求 | 出願から3年以内。なければ取下げ擬制で、故意でなければ回復できる。早期審査は審査請求済みが要件 |
| 中間処理 | 拒絶理由通知の応答期間は60日。期間内の延長（2か月）と期間経過後の延長（2か月）はどちらか一度。最後の拒絶理由通知への補正は限定される |
| 登録と維持 | 特許査定から30日以内に第1〜3年分を納付して設定登録。第4年以後は前年以前に納付し、6月以内なら割増特許料で追納できる。追納しなければ遡って消滅する |
''',
        'rdra_appendix': ('実体審査と中間処理 BUC の RDRA 全体図', 'rdra_buc_office_action.mmd'),
        'metrics_note': '実際の知的財産部で使うときは、その会社の実績で置き換える。',
        'no_flow_note': '',
    },
    'food-permit-ops': {
        'title': 'Food Permit Ops',
        # The BUC chapters in the order a business runs from the consultation to a disposition.
        'chapters': [
            'BucConsultation', 'BucPermitApplication', 'BucInspectionDecision', 'BucRenewal',
            'BucPermitChanges', 'BucBusinessNotification', 'BucMonitoring', 'BucDisposition',
        ],
        'state_tables': {'PermitApplication': 'states_permit_application.txt',
                         'BusinessPermit': 'states_business_permit.txt', 'Disposition': 'states_disposition.txt'},
        'subject': '題材は日本の保健所の食品営業許可業務（事前相談と営業の種類の判定、許可申請、施設検査、許可・不許可、有効期間と更新、変更・承継・廃業の届出、営業届出、監視指導と不利益処分）で、審査請求の中身・食品表示・輸入食品・食中毒調査・他法令の許可は対象外。',
        'overview': '''対象は都道府県・保健所設置市・特別区の保健所が行う食品の営業許可業務である。業務を 8 の BUC に分け、内部システムは `PermitLedgerSystem`（営業許可台帳システム）1 つにまとめている。国の食品衛生申請等システムは外部システムとしてイベントでつなぐ。

業務の骨格は食品衛生法 55 条の許可と、行政手続法の審査・処分の手続である。期間（有効期間、営業停止の期間、審査請求期間、公表期間）はタイマーで表し、期間が過ぎたときに起きること（失効・停止の終了・処分の確定・公表の終了）を状態で表す。標準処理期間は過ぎても状態が変わらないので NFR にしている。

| 論点 | このモデル |
|---|---|
| 許可・届出・不要 | 営業の種類で決まる。許可は施行令35条の32業種、届出は法57条、どちらも要らない営業は施行令35条の2 |
| 申請 | 窓口（紙）と電子申請（食品衛生申請等システム）が同じ状態を通る。手数料は条例で、電子申請でも別途納め、還付しない |
| 審査 | 施設検査は自治体の運用（法令の明文はない）。基準に合わなければ改善して再検査。合えば許可しなければならず、不許可には理由を示す |
| 有効期間と更新 | 5年以上で自治体が定める。満了前に継続の申請と施設検査をし、満了すれば失効して新規扱い |
| 届出 | 営業届出には施設基準も更新もない。変更・承継・廃業は届け出る |
| 処分 | 取消は聴聞、停止と整備改善命令は弁明の機会。計測で確かめた技術的基準の不充足に従えと命じる処分は意見陳述を省ける。理由を示し、審査請求（3月）を教示する |
''',
        'rdra_appendix': ('不利益処分と公表 BUC の RDRA 全体図', 'rdra_buc_disposition.mmd'),
        'metrics_note': '実際の保健所で使うときは、その自治体の実績で置き換える。',
        'no_flow_note': '',
    },
}
CONF = SAMPLES[NAME]
MODEL = f'samples/{NAME}'
SAMPLE = ROOT / MODEL
OUT = SAMPLE / 'out'
CHAPTERS = CONF['chapters']
STATE_TABLES = CONF['state_tables']


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


HEAD = f'''<!-- derived-from ../../docs/language-reference.md -->
<!-- derived-from ../../docs/cli-reference.md -->

`scripts/gen-clinic-ops-docs.py` が `{MODEL}/` のモデルから CLI を通して生成する。図・表・説明文を手で直さず、`.rdra` を直して `scripts/check-sample-artifacts.sh` を実行する。{CONF['subject']}業務規則は `.rdra` の `//` コメントに一次資料の出典（ファイル名:行）を付けている。
'''

# ── design-doc.md ──────────────────────────────────────────────────────────────

d = [f"# {CONF['title']} 設計書\n\n", HEAD, '''
この文書は実装するエンジニア向けである。構造、処理、仕様の順に読む。

1. BUC ごとのロバストネス図で、画面・制御・エンティティの構造をつかむ。
2. UC ごとのシーケンス図で、ガードと書き込みの順序をつかむ。
3. 状態モデル、データモデル、規則、CRUD と権限で仕様を確かめる。

要求と成功指標は [PRD](prd.md) にある。

## 1. 概要

''', CONF['overview'], '''
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
- ''' + CONF['rdra_appendix'][0] + f": [`out/{CONF['rdra_appendix'][1]}`](out/{CONF['rdra_appendix'][1]})" + '''

''')
d.append(folded('業務の流れ図（ビジネスコンテキスト）', mermaid('business.mmd')))
d.append('\n' + folded('全画面の画面遷移図', mermaid('screen.mmd')))

# ── prd.md ─────────────────────────────────────────────────────────────────────

in_scope = [r for r in requirements if r['status'] != 'out-of-scope']
out_of_scope = [r for r in requirements if r['status'] == 'out-of-scope']

p = [f"# {CONF['title']} PRD\n\n", HEAD, '''
この文書はプロダクトマネージャー向けである。なぜ作るか、誰のためか、何を満たせばよいかを示す。画面・API・データの設計は [設計書](design-doc.md) にある。

## 1. 背景と目的

成功指標の現状値と目標値はモデルが置いた想定の数字で、一次資料はない。''' + CONF['metrics_note'] + '''

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
この後の節は BUC ごとの業務フローである。手順、分岐、例外と、各手順を担う UC を示す。''' + CONF['no_flow_note'] + '''

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
