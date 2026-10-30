#!/usr/bin/env python3
"""Generate samples/clinic-ops/design-sample.md from the model sources and the committed out/ artifacts.

Every diagram, CSV row, and state table is read from samples/clinic-ops/out/, so the document
matches what scripts/check-sample-artifacts.sh regenerates. The prose per BUC and UC lives below,
and the script fails when a use case has no prose or belongs to no BUC listed here.

Run after scripts/check-sample-artifacts.sh: python3 scripts/gen-clinic-ops-design-sample.py
"""
import csv
import io
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SAMPLE = ROOT / 'samples/clinic-ops'
OUT = SAMPLE / 'out'

src = '\n'.join(p.read_text() for p in sorted(SAMPLE.glob('**/*.rdra')))
src = re.sub(r'//[^\n]*', '', src)


def pairs(pred):
    return re.findall(rf'^{pred}\((\w+),\s*(\w+)\)', src, re.M)


uc_label = dict(re.findall(r'^usecase (\w+)\s+"([^"]+)"', src, re.M))
contains = pairs('contains')
invokes = pairs('invokes')
req_perm = {}
for a, p in pairs('requires_permission'):
    req_perm.setdefault(a, []).append(p)
has_perm = {}
for a, p in pairs('has_permission'):
    has_perm.setdefault(a, []).append(p)


def perms(uc):
    out = list(req_perm.get(uc, []))
    for u, api in invokes:
        if u == uc:
            out += [p for p in req_perm.get(api, []) if p not in out]
    return out


def mermaid(rel):
    return '```mermaid\n' + (OUT / rel).read_text().rstrip('\n') + '\n```\n'


def gen_cmd(cmd):
    return f'生成コマンド:\n\n```sh\nrdra-ish {cmd}\n```\n'


def csv_rows(rel, pick, cols=(0, 1)):
    """The first row of the CSV whose columns `cols` equal each key, in key order."""
    text = (OUT / rel).read_text()
    rows = list(csv.reader(io.StringIO(text)))
    header, body = rows[0], rows[1:]
    chosen = [next(r for r in body if tuple(r[c] for c in cols) == key) for key in pick]
    buf = io.StringIO()
    w = csv.writer(buf, lineterminator='\n')
    w.writerow(header)
    for r in chosen:
        w.writerow(r)
    return '```csv\n' + buf.getvalue() + '```\n'


BUCS = [
    dict(id='BucPatientOnboarding', stem='patient_onboarding', title='Patient Onboarding', business='PatientAccess',
         actors='ReceptionClerk（医事課 受付担当）, Patient',
         data=['PatientAccount', 'PatientProfile', 'InsurancePolicy', 'EligibilityCheck', 'ConsentRecord', 'IntakePacket', 'PatientMessage'],
         prose='新患登録から問診送付までを扱う。新患登録では、オンライン資格確認で得た保険情報と、患者が同意した場合の限度額情報（適用区分）を `InsurancePolicy` に登録する。負担割合 `burden_ratio` と限度額適用区分 `limit_category` は actor が選ぶ値ではなく保険者が決める値なので、variation 型の属性として持つ。',
         points=['VerifyInsurance が、保険資格の登録と限度額情報の取得を同じ API 境界で扱ってよいか。',
                 '受付ごとの資格確認（`BucVisitCheckIn`）と新患登録時の資格登録の責務が混ざっていないか。',
                 'SendIntakeForms は予約イベントから起動されるため、予約 BUC 側の責務と混同しないか。']),
    dict(id='BucAppointmentScheduling', stem='appointment_scheduling', title='Appointment Scheduling', business='PatientAccess',
         actors='ReceptionClerk, Patient', data=['Appointment', 'ProviderSchedule', 'Notification', 'PatientMessage'],
         prose='予約検索、仮押さえ、予約確定、変更、取消、通知を扱う。予約枠と予約レコードを同時に更新する UC は専用 API を持ち、予約通知は AppointmentNoticeApi に分ける。予約済みの患者は、来院前に `BucVisitCheckIn` の一括照会の対象になる。',
         points=['ReserveAppointment と BookAppointment の差が業務上必要か。',
                 'CancelAppointment が Appointment と ProviderSchedule を同じ API 境界で更新することが妥当か。',
                 '予約時に選ばれる `Appointment.referral`（再診・紹介状あり・紹介状なし・除外）が、受付で確認する紹介状の有無と一致するか。']),
    dict(id='BucVisitCheckIn', stem='visit_check_in', title='受付（初診・再診）', business='PatientAccess',
         actors='ReceptionClerk, Nurse',
         data=['Appointment', 'PatientAccount', 'InsurancePolicy', 'EligibilityCheck', 'ReferralLetter', 'Room'],
         prose='到着確認、資格確認、紹介状の受付、診察室への案内を扱う。療養担当規則 第3条により受付のたびに資格を確認するので、`CheckInPatient` は必ずオンライン資格確認を伴い `EligibilityCheck` を作る。結果は 有効・無効・該当資格なし の 3 通りで、無効と該当資格なしは再確認へ戻る。システムが答えられない場合は被保険者資格申立書かシステム障害時モードで確認する。来院前には予約済み患者の一括照会を行う。受付では支払を受けない（会計は `BucVisitPayment`）。',
         points=['受付と資格確認が 1 つの UC にまとまり、資格確認を飛ばす経路がないか。',
                 '一括照会の結果が無効だった患者を、受付で再確認する流れが説明できるか。',
                 '被保険者資格申立書とシステム障害時モードが、資格確認結果の状態として区別されているか。']),
    dict(id='BucClinicalEncounter', stem='clinical_encounter', title='Clinical Encounter', business='CareDelivery',
         actors='Nurse（看護師）, Clinician（医師）', data=['Encounter', 'Appointment', 'VitalSign', 'Diagnosis', 'Room', 'ReferralLetter'],
         prose='診療記録を開き、バイタル、診断、署名、診察終了までを扱う。診察終了 `CompleteConsultation` で予約は「計算待ち」になり、会計は計算受付へ移る。他院紹介では医師が診療情報提供書を作成・送信する。EvEncounterSigned は検査、コスト登録、会計、ケアプラン作成を起動する。',
         points=['SignEncounter を後続 BUC の起点イベントとして扱ってよいか。',
                 'CompleteConsultation が予約を完了にせず「計算待ち」に置くことで、会計前に予約が閉じないか。',
                 'IssueReferralLetter（診療情報提供書）と受付の紹介状受付（ReceiveReferralLetter）が同じ `ReferralLetter` を向きで区別できるか。']),
    dict(id='BucOrdersResults', stem='orders_results', title='Orders and Results', business='CareDelivery',
         actors='Clinician, Nurse, MedicalClerk（医師事務作業補助者）', data=['ClinicalOrder', 'LabResult', 'Charge', 'Notification', 'PatientMessage'],
         prose='検査オーダーから結果確認、重要結果通知までを扱う。医師事務作業補助者や看護師がオーダを代行入力した場合、医師が確定するまで看護師は採血しない（保助看法 第37条）。この規則は `OrderConfirmedByPhysician` 条件と `forbidden(ClinicalOrder, status == clcollected, physician_confirmed == false)` で検証する。外注検査の結果は外注検査システムから届く。採血管ラベルの発行と実施内容（コスト）の登録が会計計算の入力になる。',
         points=['代行入力オーダが医師の確定前に採血へ進まないことが、状態検証で示されているか。',
                 'RegisterPerformedProcedure が作る Charge を、会計計算入力が読めるか。',
                 'NotifyCriticalResult は結果確認イベントから起動されるため、手動 UC と自動 UC の違いをレビューできるか。']),
    dict(id='BucPrescriptionFulfillment', stem='prescription_fulfillment', title='処方', business='CareDelivery',
         actors='Clinician（医師）, Pharmacist（薬剤師）', data=['Prescription', 'Medication', 'Encounter'],
         prose='処方オーダから院外処方箋の発行、院内処方の調剤・監査・服薬指導・交付、疑義照会までを扱う。院外処方箋は電子処方箋管理サービスへ登録し、患者が選ぶ保険薬局で調剤される。病院は特定の保険薬局を指示しない（療担規則 第2条の5）ので、`Prescription` は薬局を持たない。使用期間は交付日を含めて4日以内で、日数は DSL で数えられないためタイマーで期限切れにする。薬剤師は疑わしい点があれば医師に問い合わせ、回答までは調剤しない。',
         points=['院外（rxissued）と院内（rxvoucher → rxaudited → rxcounseled → rxdispensed）の経路が状態で区別できるか。',
                 '疑義照会中（rxinquiry）の処方箋が調剤・監査に進まないか。',
                 '使用期間切れ（rxexpired）が院外処方箋にだけ起きることが業務と合うか。']),
    dict(id='BucVisitPayment', stem='visit_payment', title='計算・会計', business='RevenueCycle',
         actors='AccountsClerk（医事課 会計担当）', data=['Appointment', 'Charge', 'PaymentTransaction', 'AccountBalance', 'InsurancePolicy', 'SelectedTreatmentFeeSetting'],
         prose='診察の後の会計計算入力、患者の呼出、会計窓口または自動精算機での精算、領収証・明細書の交付を扱う。一部負担金は負担割合で計算し、限度額情報があれば窓口での支払は自己負担限度額までになる（`CopayCappedAtLimit`）。紹介状なしの初診で除外に当たらない場合は選定療養費を自費の Charge として算定する（`UnreferredFirstVisit`）。金額は病院の設定 `SelectedTreatmentFeeSetting` に置き、モデルは数値を持たない。',
         points=['予約が 計算待ち → 会計待ち → 精算済 → 完了 と進み、受付時点で支払が発生しないか。',
                 '選定療養費の除外（救急搬送、初診時にそのまま入院 など）が `ReferralCategory` の値として説明できるか。',
                 '窓口精算と自動精算機の精算が同じ PaymentTransaction と未収金残高を更新してよいか。']),
    dict(id='BucBillingClaims', stem='billing_claims', title='レセプト作成・請求', business='RevenueCycle',
         actors='ClaimsClerk（医事課 レセプト担当）', data=['ClaimStatement', 'Charge', 'InsurancePolicy', 'AccountBalance'],
         prose='レセプト（患者 × 診療月 × 保険者）の作成・院内点検・請求と、審査支払機関（支払基金・国保連）からの受付不能・返戻・査定・支払への対応を扱う。前月分は翌月10日までに請求し、受付不能・要確認は11日・12日に再送する。返戻は訂正して当月分と併せて再請求できるが、査定は再請求できず再審査請求で争う。資格が変わったレセプトは審査支払機関が振替・分割するので返戻にならないが、負担割合が異なる変更や公費の分割はできず返戻になる。支払は診療の翌々月。期間のうち DSL で表せないもの（10日、11・12日、5日〜月末、3か月、翌々月）はタイマーとコメントで示す。',
         points=['返戻（stmtreturned → stmtinspecting）と査定（stmtassessed → stmtrereview）の出口が区別されているか。',
                 '保留（月遅れ）と受付不能が、それぞれ請求前と請求後の状態として説明できるか。',
                 'GenerateClaimStatement が Charge を更新しない（読むだけ）ことで、レセプトの検証が受診の状態から切り離されていることが妥当か。']),
    dict(id='BucFollowupCare', stem='followup_care', title='Follow-Up Care', business='PatientAccess',
         actors='CareCoordinator, Patient', data=['CarePlan', 'PatientMessage', 'Appointment', 'Notification'],
         prose='診療後のケアプラン作成、患者連絡、患者応答確認、再診予約、ケアプラン終了を扱う。',
         points=['ScheduleFollowUpVisit が予約 BUC と同じ Appointment lifecycle を使ってよいか。',
                 'NotifyPatientResult と NotifyCriticalResult の通知責務が重複していないか。']),
    dict(id='BucStaffAdministration', stem='staff_administration', title='Staff Administration', business='ClinicAdministration',
         actors='ClinicAdmin', data=['Provider', 'ProviderSchedule', 'Room', 'AuditEvent'],
         prose='診療所運営のマスタ・監査系業務を扱う。',
         points=['ReleaseRoom と診察終了の部屋状態更新が重複しないか。',
                 'ResolveAuditFinding が AdminConsole に限定されているか。']),
]

POINTS = {
    'SearchPatient': '患者検索が read-only API として表現され、患者作成と混ざらないこと',
    'RegisterPatient': 'PatientAccount と PatientProfile を同時作成する境界',
    'UpdateDemographics': '患者属性更新の API 境界',
    'VerifyInsurance': '保険資格の登録と限度額情報（適用区分）の取得',
    'CaptureConsent': '同意記録作成と nullable/Bool effect',
    'SendIntakeForms': '問診パケットとメッセージ作成',
    'CompleteIntakeForms': '問診完了イベント',
    'ExpireIntakeForms': '問診期限切れイベント',
    'MergeDuplicatePatient': '患者統合状態',
    'ArchivePatient': '患者アーカイブ状態',
    'SearchAvailability': '予約枠検索の read-only API',
    'ReserveAppointment': '仮押さえで Appointment を作る境界',
    'BookAppointment': '予約確定と予約枠更新',
    'RescheduleAppointment': '予約変更の自己遷移',
    'CancelAppointment': '取消理由と予約枠更新',
    'MarkNoShow': 'タイマー起動の未来院処理',
    'SendAppointmentNotice': '予約イベントからの通知作成',
    'VerifyArrival': '予約票・診察券の確認（参照のみ）',
    'CheckInPatient': '受付で予約を受付済にし、同時にオンライン資格確認を作る',
    'BatchVerifyEligibility': '予約済み患者（BookedPatient 条件）だけを一括照会する',
    'ReceiveEligibilityValid': '資格確認結果「有効」の取込',
    'ReceiveEligibilityInvalid': '資格確認結果「無効」の取込',
    'ReceiveEligibilityNoInfo': '資格確認結果「該当資格なし」の取込',
    'RecheckEligibility': '無効・該当資格なしからの再確認',
    'RecordEligibilityDeclaration': '被保険者資格申立書による確認',
    'ConfirmEligibilityInOutageMode': 'システム障害時モードによる確認',
    'ReceiveReferralLetter': '紹介状の受領と受付処理',
    'AssignRoom': '部屋と予約の同期更新',
    'PrepareEncounter': '診療準備の参照情報',
    'OpenEncounter': '受付済の予約から診療記録を開く',
    'RecordVitals': 'バイタル記録',
    'DocumentAssessment': '診断記録と記録状態',
    'SignEncounter': '署名と後続 BUC の起点イベント',
    'AmendEncounter': '署名済み記録の修正',
    'IssueReferralLetter': '診療情報提供書の作成・送信',
    'CompleteConsultation': '診察終了で予約を計算待ちにし、部屋を清掃中にする',
    'PlaceLabOrder': '医師が登録したオーダは確定済み',
    'EnterProxyOrder': '代行入力オーダは未確定で作られる',
    'ConfirmProxyOrder': '医師による代行入力オーダの確定',
    'PrintSpecimenLabel': '採血管ラベルの発行（参照のみ）',
    'CollectSpecimen': '医師が確定したオーダだけを採取する',
    'RegisterPerformedProcedure': '実施内容（コスト）の登録',
    'ReceiveLabResult': '外注検査システムからの結果受信',
    'ReviewLabResult': '結果確認と後続通知の起点',
    'NotifyCriticalResult': '重要結果の患者通知',
    'CancelClinicalOrder': 'オーダ取消',
    'SearchMedication': '薬剤検索の read-only API',
    'DraftPrescription': '処方オーダの作成',
    'IssueRefillPrescription': 'リフィル処方箋の処方オーダ（使用回数の上限を記載）',
    'IssueOutsidePrescription': '院外処方箋の発行と電子処方箋管理サービスへの登録',
    'IssueInhouseVoucher': '院内処方用引換券の交付',
    'DispenseAndAudit': '薬剤師による調剤・監査',
    'CounselMedication': '服薬指導',
    'HandOverMedication': '薬剤交付',
    'RaisePrescriptionInquiry': '薬剤師による疑義照会',
    'AnswerPrescriptionInquiry': '医師の回答で引換券へ戻す',
    'AnswerPharmacyInquiry': '保険薬局からの疑義照会への回答',
    'ConfirmDispense': '保険薬局での調剤結果の受信',
    'ExpirePrescription': '使用期間切れのタイマー処理',
    'CancelPrescription': '処方の取消',
    'CalculateVisitBill': '会計計算入力で予約を会計待ちにする',
    'ApplySelectedTreatmentFee': '紹介状なし初診だけに選定療養費の Charge を作る',
    'CallPatientForPayment': '会計の呼出（参照のみ）',
    'SettleAtWindow': '会計窓口での精算',
    'SettleAtKiosk': '自動精算機での精算',
    'IssueReceipt': '領収証・明細書の交付で予約を完了にする',
    'CreateCharge': '署名時のコスト情報登録（システム起動）',
    'GenerateClaimStatement': '月初のレセプト作成',
    'InspectClaimStatement': '院内点検',
    'HoldClaimStatement': '保留（月遅れ）',
    'ReleaseHeldStatement': '保留レセプトの再提出準備',
    'SubmitClaimStatements': '毎月10日までの請求',
    'RecordReceiptError': '受付不能・要確認の確認',
    'ResendErroredStatement': '11日・12日のエラー分再送',
    'ReceiveClaimAccepted': '審査支払機関の受付で審査中になる',
    'ReviewClaimTransfer': '資格の振替・分割の確認（参照のみ）',
    'RecordClaimReturn': '返戻レセプトの取込（3か月以内）',
    'CorrectReturnedStatement': '返戻の訂正と再請求',
    'RecordClaimAssessment': '査定の取込',
    'RequestReexamination': '再審査請求',
    'PostPayment': '診療翌々月の入金',
    'ReconcileBalance': '未収金管理台帳の作成',
    'VoidCharge': 'コストの取消',
    'CreateCarePlan': '署名イベントからのケアプラン作成',
    'SendFollowUpMessage': '患者連絡',
    'ReviewPatientResponse': '患者応答の確認',
    'ScheduleFollowUpVisit': '再診予約',
    'CloseCarePlan': 'ケアプラン終了',
    'NotifyPatientResult': '結果の患者通知',
    'ManageProviderSchedule': '診療枠の管理',
    'BlockScheduleSlot': '診療枠の停止',
    'ConfigureRoom': '部屋の設定',
    'ReleaseRoom': '部屋の解放',
    'ReviewAuditEvents': '監査イベントの確認',
    'ResolveAuditFinding': '監査指摘の解決',
}


def buc_ucs(buc):
    return [b for a, b in contains if a == buc and b in uc_label]


def buc_perms(buc):
    out = []
    for uc in buc_ucs(buc):
        out += [p for p in perms(uc) if p not in out]
    return out


def code_list(xs):
    return ', '.join(f'`{x}`' for x in xs) if xs else '（なし）'


parts = []
parts.append('''# Clinic Ops 設計書サンプル

<!-- derived-from ../../docs/language-reference.md -->
<!-- derived-from ../../docs/cli-reference.md -->
<!-- derived-from ../../docs/state-derivation.md -->

この文書は `scripts/gen-clinic-ops-design-sample.py` が `samples/clinic-ops/` の `.rdra` と `out/` から生成します。図・CSV・状態到達表を手で直さず、モデルを直して `scripts/check-sample-artifacts.sh` の後にこのスクリプトを実行してください。

この文書は `samples/clinic-ops/` を題材に、要求整理、BUC 分解、UC 分解、System/API 境界、データモデリング、状態検証をレビューできる粒度でまとめる設計書サンプルです。題材は日本の病院の外来業務（受付、資格確認、診察、検査、処方、計算・会計、レセプト請求）で、業務規則は `.rdra` の `//` コメントに一次資料の出典（ファイル名:行）を付けています。入院は対象外です。

図は外部リンクではなく本文に埋め込みます。レビューでは対象章だけを開き、BUC 単位、UC 単位、System/API 単位、データモデル単位の順に確認します。

## 1. 成果物一覧

| 種別 | 生成先 | 用途 |
|---|---|---|
| BUC 別 RDRA Layered Graph 図 | `out/buc/object_graph_*.mmd` | 各 BUC の actor/usecase/entity/screen/event を確認する |
| API マトリクス | `out/api_matrix.csv` | API と Entity CRUD の棚卸し |
| Screen 制約パターン | `out/screen_constraints.csv` | Screen に到達する UC/API ごとの権限・媒体制約を確認する |
| Actor 権限割当監査 | `out/actor_permission_audit.csv` | Actor 側の権限付与が UC/API 要求と合っているかを確認する |
| Event flow | `out/event_flow.mmd` | BUC 間イベント連鎖を確認する |
| ER 図 | `out/er_care_to_billing.mmd` | 診療から請求までのデータ関係を確認する |
| State 図 | `out/state_whole.mmd` | 主要状態遷移の全体像を確認する |
| 状態到達表 | `out/states_appointment.txt`, `out/states_claim_statement.txt` | lifecycle の到達可能性を確認する |

生成コマンド例:

```sh
rdra-ish diagram samples/clinic-ops --kind rdra --format mermaid --buc BucClinicalEncounter --out samples/clinic-ops/out/buc/object_graph_clinical_encounter
rdra-ish csv samples/clinic-ops --kind api-matrix --out samples/clinic-ops/out/api_matrix.csv
rdra-ish csv samples/clinic-ops --kind screen-constraints --out samples/clinic-ops/out/screen_constraints.csv
rdra-ish csv samples/clinic-ops --kind actor-permission-audit --out samples/clinic-ops/out/actor_permission_audit.csv
```

## 2. 業務スコープ

対象は病院の外来業務です。患者登録、予約、受付、診療、検査、処方、計算・会計、レセプト請求、フォローアップ、運営管理を BUC として分け、業務担当者がレビューしやすい単位にしています。受付・会計・レセプトは医事課の業務で、病院が外部に委託することもあります。

| Business | BUC | 主な actor | 目的 |
|---|---|---|---|
''')
for b in BUCS:
    parts.append(f"| `{b['business']}` | `{b['id']}` | {b['actors']} | {b['title']} |\n")
parts.append('''
米国型の診療所モデルとの主な違いは次のとおりです。

| 論点 | このモデル |
|---|---|
| 支払の時点 | 受付では支払わない。診察の後に計算受付・会計受付で精算する |
| 窓口負担 | 負担割合（3割・2割）で計算し、限度額情報があれば自己負担限度額までを窓口で受け取る |
| 資格確認 | 受付のたびにオンライン資格確認等システムで確認する。来院前の一括照会、申立書、システム障害時モードがある |
| 請求 | レセプトを月ごとに審査支払機関（支払基金・国保連）へ請求する。返戻は再請求でき、査定は再審査請求で争う |
| 処方 | 院外処方箋は電子処方箋管理サービスへ登録し、患者が選ぶ保険薬局で調剤する。院内処方は薬剤師が調剤・監査する |
| 紹介 | 紹介状なしの初診は選定療養費（自費）の対象。救急搬送などは除外 |

レビューでは、まず BUC の責務境界を確認し、次に UC 単位で API/Entity 操作を確認します。最後に System/API マトリクスと ER/状態検証で横断的な整合性を見ます。

### 2.1 Context / Access 制約

権限語彙と媒体語彙は `shared/biz.rdra`、actor への権限割当は
`shared/access.rdra` に分けている。各 BUC ファイルでは、UC/API 側の要求を
`requires_permission` / `requires_medium` として定義する。

`BucAppointmentScheduling` は `belongs(...).by(...)` で、
予約業務が窓口端末または患者モバイルから発生することを表す。`BucVisitPayment` は
`AccountsWindow`（会計窓口）と `PaymentKiosk`（自動精算機）、`BucBillingClaims` は
`BillingWorkstation` と `OnlineClaimTerminal`（オンライン請求用端末）から発生する。

Screen には制約を直接書かない。`displays(UC, Screen)` と `invokes(UC, Api)` から
次のような行が `out/screen_constraints.csv` に導出される。

''')
parts.append(csv_rows('screen_constraints.csv', [
    ('ConsentScreen', 'CaptureConsent'), ('AppointmentScreen', 'BookAppointment'),
    ('CheckInScreen', 'CheckInPatient'), ('PaymentWindowScreen', 'SettleAtWindow'),
    ('ClaimScreen', 'SubmitClaimStatements'), ('DispensingScreen', 'DispenseAndAudit')]))
parts.append('''
Actor 側の権限付与は `out/actor_permission_audit.csv` で確認する。`missing` は
UC/API 要求に対して actor の `has_permission` が不足していること、`excess` は現行モデル上
その actor の実行経路で使われない権限が付いていることを示す。

## 3. BUC 単位の設計レビュー

''')
for i, b in enumerate(BUCS, 1):
    parts.append(f"### 3.{i} {b['title']}\n\n| 項目 | 内容 |\n|---|---|\n")
    parts.append(f"| BUC | `{b['id']}` |\n| Business | `{b['business']}` |\n| 主な actor | {b['actors']} |\n")
    parts.append(f"| 必要権限 | {code_list(buc_perms(b['id']))} |\n| 主なデータ | {code_list(b['data'])} |\n\n")
    parts.append(b['prose'] + '\n\nレビュー観点:\n\n' + ''.join(f'- {p}\n' for p in b['points']) + '\n')
    parts.append(f"#### {b['title']} RDRA Layered Graph 図\n\n")
    parts.append(gen_cmd(f"diagram samples/clinic-ops --kind rdra --format mermaid --buc {b['id']} --out samples/clinic-ops/out/buc/object_graph_{b['stem']}") + '\n')
    parts.append(mermaid(f"buc/object_graph_{b['stem']}.mmd") + '\n')

parts.append('''## 4. UC 単位のレビュー

UC ごとの必要権限と確認点を表にまとめます。各 UC の画面、API、Entity 操作は BUC 別 RDRA Layered Graph 図と API マトリクスで確認します。

''')
for i, b in enumerate(BUCS, 1):
    ucs = buc_ucs(b['id'])
    parts.append(f"### 4.{i} {b['title']} UC\n\n| UC | 必要権限 | 主な確認点 |\n|---|---|---|\n")
    for uc in ucs:
        parts.append(f"| `{uc}` | {code_list(perms(uc))} | {POINTS[uc]} |\n")
    parts.append('\n')

parts.append('''## 5. System/API 単位の設計

### 5.1 System 境界

本サンプルでは、内部 System を `ClinicOpsSystem` に集約しています。現時点の意図は、分散システム境界よりも API と Entity CRUD の棚卸しをレビューすることです。院外の仕組みは外部システムとしてイベントでつなぎます。

| 外部システム | 役割 |
|---|---|
| `OnlineEligibilitySystem` | オンライン資格確認等システム。受付と一括照会の資格確認結果を返す |
| `PaymentGateway` | 自動精算機・クレジット決済。精算完了を通知する |
| `OutsourcedLabSystem` | 外注検査システム。検査結果を返す |
| `EPrescriptionService` | 電子処方箋管理サービス。院外処方箋を受け、保険薬局での調剤結果を返す |
| `InsurancePharmacy` | 保険薬局。疑義照会を送る |
| `ClaimsReviewOrganization` | 審査支払機関（支払基金・国保連）。受付不能、受付、振替・分割、返戻、査定、支払を返す |
| `MessagingGateway` | 患者への通知 |

```rdra
system ClinicOpsSystem "Clinic Operations System"
contains(ClinicOpsSystem, <EachApi>)
```

### 5.2 API 設計方針

| 方針 | 理由 |
|---|---|
| 書き込み UC は原則 UC 専用 API を持つ | API matrix で UC ごとの書き込み責務が混ざらないようにする |
| read-only UC も必要に応じて API 化する | API matrix で参照責務を確認する |
| direct CRUD は単純な状態更新に限定する | API 境界を置くほどの一貫性責務がない場合のみ使用する |
| 作成 API と更新 API を分ける | 作成 API を呼ぶ UC は空きスロットを要するので、状態だけを動かす UC には更新 API を使う |

### 5.3 Event flow

Event flow は BUC をまたぐ連鎖だけを見るための図です。

''')
parts.append(gen_cmd('diagram samples/clinic-ops --kind event --format mermaid --out samples/clinic-ops/out/event_flow') + '\n')
parts.append(mermaid('event_flow.mmd') + '\n')
parts.append('''主要イベントと起動先:

| Event | 発生元 | 起動先 |
|---|---|---|
| `EvAppointmentCheckedIn` | `CheckInPatient` | `OnlineEligibilitySystem`, `BucClinicalEncounter`, `OpenEncounter`, `PrepareEncounter` |
| `EvEligibilityChecked` | `OnlineEligibilitySystem` | `ReceiveEligibilityValid`, `ReceiveEligibilityInvalid`, `ReceiveEligibilityNoInfo` |
| `EvEncounterSigned` | `SignEncounter` | `BucBillingClaims`, `BucVisitPayment`, `BucFollowupCare`, `BucOrdersResults`, `CompleteConsultation`, `PlaceLabOrder`, `CreateCharge`, `CreateCarePlan` |
| `EvBillCalculated` | `CalculateVisitBill` | `CallPatientForPayment`, `PaymentGateway` |
| `EvPaymentSettled` | `PaymentGateway` | `SettleAtKiosk` |
| `EvClaimStatementsSubmitted` | `SubmitClaimStatements` | `ClaimsReviewOrganization` |
| `EvClaimReturned` | `ClaimsReviewOrganization` | `RecordClaimReturn` |
| `EvClaimAssessed` | `ClaimsReviewOrganization` | `RecordClaimAssessment` |
| `EvRemittance` | `ClaimsReviewOrganization` | `PostPayment` |
| `EvPharmacyInquiry` | `InsurancePharmacy` | `AnswerPharmacyInquiry` |

タイマーと起動先:

| Timer | 起動先 | 根拠となる期間 |
|---|---|---|
| `EligibilityBatchInquiry` | `BatchVerifyEligibility` | 来院前（予約済み患者） |
| `MonthlyClaimCreation` | `GenerateClaimStatement` | 前月分を月初に作成 |
| `MonthlyClaimDeadline` | `SubmitClaimStatements` | 翌月10日まで、請求可能期間 5日～10日 |
| `ErrorResendWindow` | `ResendErroredStatement` | エラー分は 11日・12日 |
| `ReturnDownloadCheck` | `RecordClaimReturn` | 返戻ファイルのダウンロード期間 3か月 |
| `PrescriptionUsePeriodCheck` | `ExpirePrescription` | 交付の日を含めて4日以内 |
| `ReceivablesMonthlyReview` | `ReconcileBalance` | 未収金管理台帳（毎月） |

再審査等請求期間（5日～月末）と支払の時期（診療翌々月）は、use case を起動する時点ではないため `.rdra` のコメントに出典付きで書いている。

### 5.4 API マトリクスの読み方

`out/api_matrix.csv` は、API を行、Entity を列にした CRUD 表です。レビューでは、1 API が不自然に多くの Entity を操作していないか、read-only API と write API が混ざっていないか、複数 Entity 更新が業務上 1 つの atomic boundary として説明できるかを確認します。

## 6. 最終データモデリング

### 6.1 データ領域

| 領域 | Entity |
|---|---|
| 患者マスタ | `PatientAccount`, `PatientProfile`, `InsurancePolicy`, `EligibilityCheck`, `ConsentRecord`, `IntakePacket`, `ReferralLetter` |
| 施設・予定 | `Provider`, `ClinicLocation`, `Room`, `ProviderSchedule` |
| 予約・診療 | `Appointment`, `Encounter`, `VitalSign`, `Diagnosis` |
| オーダー・結果・処方 | `ClinicalOrder`, `LabResult`, `Medication`, `Prescription` |
| 会計 | `Charge`, `PaymentTransaction`, `AccountBalance`, `SelectedTreatmentFeeSetting` |
| 請求 | `ClaimStatement` |
| フォロー・通知・監査 | `CarePlan`, `PatientMessage`, `Notification`, `AuditEvent` |

variation（actor や保険者が決め、use case が動かさない値）:

| Variation | 型を付ける列 | 値 |
|---|---|---|
| `BurdenRatio` | `InsurancePolicy.burden_ratio` | 3割, 2割 |
| `LimitCategory` | `InsurancePolicy.limit_category` | 限度額情報なし, 適用区分を取得済 |
| `ReferralCategory` | `Appointment.referral` | 再診, 紹介状あり初診, 紹介状なし初診, 紹介状なし初診（救急搬送等の除外） |
| `ChargeKind` | `Charge.kind` | 保険診療, 選定療養（自費） |
| `ReferralDirection` | `ReferralLetter.direction` | 紹介元から受領, 紹介先へ送付 |
| `PayerChange` | `ClaimStatement.payer_change` | 資格変更なし, 負担割合が同じ資格への変更, 負担割合が異なる資格への変更, 公費が関わる変更 |
| `PaymentMethod` | `PaymentTransaction.method` | 現金, クレジットカード, 還付・追加徴収 |

### 6.2 ER 図

''')
parts.append(gen_cmd('diagram samples/clinic-ops --kind er --format mermaid --buc BucFollowupCare --buc BucBillingClaims --out samples/clinic-ops/out/er_care_to_billing') + '\n')
parts.append(mermaid('er_care_to_billing.mmd') + '\n')
parts.append('### 6.3 State 図\n\n')
parts.append(gen_cmd('diagram samples/clinic-ops --kind state --format mermaid --out samples/clinic-ops/out/state_whole') + '\n')
parts.append(mermaid('state_whole.mmd') + '\n')
parts.append('### 6.4 状態到達表\n\nAppointment:\n\n```text\n' + (OUT / 'states_appointment.txt').read_text().rstrip('\n') + '\n```\n\n')
parts.append('ClaimStatement（レセプト）:\n\n```text\n' + (OUT / 'states_claim_statement.txt').read_text().rstrip('\n') + '\n```\n\n')
parts.append('''状態検証では、初期状態から想定する終端状態へ到達できるか、到達してはいけない状態組み合わせがないか、nullable column や Bool column の `sets` が不足していないかを確認します。`rdra-ish check samples/clinic-ops` と `rdra-ish verify samples/clinic-ops` は、各状態の到達証拠（NoWitness なし）、抜け出せない状態（Deadlock なし）、実行されない use case（NeverRuns なし）、invariant・forbidden・1:1 の一意性を検証する。

検証で確かめている業務規則:

| 規則 | 宣言 |
|---|---|
| 精算前に予約が完了しない | `Appointment` の状態 `apptawaitingcalc → apptawaitingpay → apptsettled → apptcompleted` と `invariant(Appointment).when(status == apptcompleted).then(completed_at == present)` |
| 医師が確定していないオーダは採取しない | `forbidden(ClinicalOrder, status == clcollected, physician_confirmed == false)` と `OrderConfirmedByPhysician` |
| 返戻の理由は再請求前に消す | `forbidden(ClaimStatement, status == stmtpaid, return_reason == present)` |
| 支払済のレセプトは入金日を持つ | `invariant(ClaimStatement).when(status == stmtpaid).then(paid_at == present)` |
| 院外処方箋は送信日時を持つ | `invariant(Prescription).when(status == rxissued).then(sent_at == present)` |
| 選定療養費は紹介状なし初診だけに付く | `UnreferredFirstVisit` が `ApplySelectedTreatmentFee` の作成を制約する |

## 7. 権限モデルレビュー

この章は、`shared/access.rdra` と各 BUC の `requires_permission` /
`requires_medium`、`out/screen_constraints.csv`、`out/actor_permission_audit.csv` を
使った権限レビューの例である。
権限は UI 画面へ直接付けず、actor が持つ権限、UC/API が要求する権限、操作媒体を分けて確認する。

### 7.1 権限語彙と actor 割当

権限語彙は業務能力の単位で切っている。画面名や API 名をそのまま権限名にせず、
「何を許可するか」が読める名前にする。

| Actor | 権限 |
|---|---|
''')
for actor, ps in has_perm.items():
    parts.append(f"| `{actor}` | {code_list(ps)} |\n")
parts.append('''
レビューでは、actor に付いた権限が BUC の責務と合っているかを先に見る。たとえば
`ReceptionClerk` は受付と資格確認を広く扱うが、会計の収納やレセプト請求は持たない。
`MedicalClerk`（医師事務作業補助者）は代行入力 `ProxyOrderEntry` だけを持ち、オーダの確定
`ClinicalOrderWrite` は医師が持つ。`Pharmacist` は調剤 `PrescriptionDispense` を持つが処方の作成は持たない。

### 7.2 UC/API 要求の読み方

UC には「その操作を開始するのに必要な権限」と「操作媒体」を置く。API には
「バックエンド境界が要求する権限」を置く。同じ UC が API を呼ぶ場合、screen constraint
行では UC と API の要求が結合される。

| BUC | 代表 UC/API | 要求 |
|---|---|---|
| Appointment Scheduling | `BookAppointment` / `BookAppointmentApi` | `ScheduleWrite`、`FrontDeskTerminal` または `PatientMobile` |
| 受付 | `CheckInPatient` / `OnlineEligibilityApi` | `VisitCheckIn` と `InsuranceVerify`、`FrontDeskTerminal` |
| Clinical Encounter | `SignEncounter` / `SignEncounterApi` | `EncounterSign`、`ClinicalWorkstation` |
| 計算・会計 | `SettleAtWindow` / `SettlementApi` | `PaymentCollect`、`AccountsWindow` |
| レセプト作成・請求 | `SubmitClaimStatements` / `SubmitClaimApi` | `BillingClaimWrite`、`OnlineClaimTerminal` |
| Staff Administration | `ResolveAuditFinding` | `AuditResolve`、`AdminConsole` |

権限レビューでは、UC の要求と API の要求が同じである必要はない。たとえば
`CheckInPatient` は受付の権限に加えて、オンライン資格確認 API 側の `InsuranceVerify`
も必要になる。

### 7.3 Screen 制約パターン

`screen_constraints.csv` は、画面を通る UC/API パスごとの権限と媒体を横断確認するための
成果物である。生成コマンド:

```sh
rdra-ish csv samples/clinic-ops --kind screen-constraints --out samples/clinic-ops/out/screen_constraints.csv
```

代表行:

''')
parts.append(csv_rows('screen_constraints.csv', [
    ('AppointmentScreen', 'BookAppointment'), ('DiagnosisScreen', 'DocumentAssessment'),
    ('CriticalNoticeScreen', 'NotifyCriticalResult'), ('KioskScreen', 'SettleAtKiosk'),
    ('PaymentPostingScreen', 'PostPayment'), ('AuditReviewScreen', 'ResolveAuditFinding')]))
parts.append('''
読み方:

- `api_id` が空の行は、UC が直接 entity を操作している。画面レビューでは UC 側の
  `requires_*` だけが根拠になる。
- `required_permissions` が `A|B` の場合は、UC/API パス上で複数の権限が必要である。
- `required_media` が `A|B` の場合は、同じ操作が複数媒体から許可される。
- 同じ screen に複数 UC が出る場合、画面単位で最も強い権限に寄せるのではなく、
  UC/API パスごとにボタン、導線、API 呼び出し制御を分けてレビューする。

### 7.4 Actor 権限割当監査

`actor_permission_audit.csv` は、`performs` で actor が実行できる UC/API パスから
必要権限を逆算し、`has_permission` と比較する成果物である。生成コマンド:

```sh
rdra-ish csv samples/clinic-ops --kind actor-permission-audit --out samples/clinic-ops/out/actor_permission_audit.csv
```

代表行:

''')
parts.append(csv_rows('actor_permission_audit.csv', [
    ('Clinician', 'PatientMessageSend'), ('ReceptionClerk', 'PatientMessageSend'),
    ('MedicalClerk', 'ProxyOrderEntry'), ('ClaimsClerk', 'BillingClaimWrite')], cols=(0, 2)))
parts.append('''
読み方:

- `missing` は、actor が実行できる UC/API パスで必要な権限が未付与である。
- `excess` は、actor に付与されているが現行モデル上の実行経路では要求されない権限である。
- `ok` は、actor 側付与と UC/API 側要求が少なくとも 1 つの実行経路で対応している。

### 7.5 レビュー観点と指摘例

| 観点 | 確認すること | 指摘例 |
|---|---|---|
| Actor coverage | `performs` する actor が必要権限を持つか | `Patient` が `BookAppointment` を実行するなら `ScheduleWrite` を持つべきか、患者用の別権限へ分けるべきか確認する |
| Least privilege | actor に不要な権限がないか | `MedicalClerk` に `ClinicalOrderWrite` を付けない（確定は医師） |
| Media constraint | 操作媒体が業務実態と合うか | レセプト請求はオンライン請求用端末に限定する |
| UC/API split | UC と API の権限が混ざっていないか | 受付 UC には `VisitCheckIn`、資格確認 API には `InsuranceVerify` を置く |
| Screen path | 同じ画面の操作ごとに制約を区別できるか | `ClaimScreen` の点検、保留、請求を同じ権限として扱ってよいか確認する |

レビューでは `shared/access.rdra`、各 BUC ファイルの Access constraints ブロック、
`out/screen_constraints.csv`、`out/actor_permission_audit.csv` を突き合わせ、
actor が実行する UC/API パスに必要な権限と媒体が説明できることを承認条件にする。

## 8. レビュー手順

1. BUC 単位の RDRA Layered Graph 図でスコープと actor/usecase を確認する。
2. 論点のある UC は UC 単位のレビュー表で確認点を確認する。
3. Event flow で BUC 間連鎖とタイマーを確認する。
4. API matrix で API/Entity CRUD の責務を横断確認する。
5. Screen 制約パターンで権限と媒体を横断確認する。
6. Actor 権限割当監査で不足/過剰な actor 側権限を確認する。
7. ER 図で最終データ構造を確認する。
8. State 図と状態到達表で lifecycle と rule の不足を確認する。
9. `.rdra` のコメントの出典で、各業務規則が一次資料に基づくことを確認する。

## 9. 承認条件

| 観点 | 承認条件 |
|---|---|
| BUC | 業務スコープが 10 BUC に分解され、担当 actor が説明できる |
| UC | 各 UC の主要 CRUD、参照、イベントが説明できる |
| API | API matrix 上で責務の広すぎる API がない |
| Access | actor 権限、UC/API 要求、媒体制約が screen constraint 行と actor permission audit 行として説明できる |
| Event | BUC 間イベント連鎖とタイマーが Event flow で説明できる |
| Data | ER と状態到達表で主要 lifecycle が確認できる |
| 出典 | 業務規則ごとに一次資料の出典が `.rdra` のコメントにある |

## Summary

<!-- derived-from #3-buc-単位の設計レビュー -->
<!-- derived-from #4-uc-単位のレビュー -->
<!-- derived-from #5-systemapi-単位の設計 -->
<!-- derived-from #6-最終データモデリング -->
<!-- derived-from #7-権限モデルレビュー -->

Clinic Ops の設計レビューは、巨大な全体図ではなく、BUC、UC、System/API、権限、データモデルの順に分割して進める。BUC 別の図は BUC の対象内に閉じ、BUC をまたぐ連鎖は Event flow で別に確認する。権限と媒体は screen constraint CSV で UC/API パスごとに確認し、actor 側の不足/過剰付与は actor permission audit で確認する。
''')

missing = [uc for uc in uc_label if uc not in POINTS]
assert not missing, missing
covered = {uc for b in BUCS for uc in buc_ucs(b['id'])}
assert covered == set(uc_label), set(uc_label) - covered
(SAMPLE / 'design-sample.md').write_text('\n'.join(l.rstrip() for l in ''.join(parts).split('\n')))
