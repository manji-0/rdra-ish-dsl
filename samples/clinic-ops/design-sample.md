# Clinic Ops 設計書サンプル

<!-- derived-from ../../docs/language-reference.md -->
<!-- derived-from ../../docs/cli-reference.md -->
<!-- derived-from ../../docs/state-derivation.md -->

この文書は `samples/clinic-ops/` を題材に、要求整理、BUC 分解、UC 分解、System/API 境界、データモデリング、状態検証をレビューできる粒度でまとめる設計書サンプルです。

以前の単一巨大図ではレビューしづらいため、図は BUC 単位・UC 単位・関心領域単位へ分割しています。全体像を一枚で説明するのではなく、レビュー会では対象章の小さい図を開く前提です。

## 1. 成果物一覧

| 種別 | ファイル | 用途 |
|---|---|---|
| BUC 別 RDRA 図 | [out/buc/](out/buc/) | 各 BUC の actor/usecase/entity/screen/event を確認する |
| BUC 別 sequence 図 | [out/buc/](out/buc/) | BUC 内の書き込み系 UC と API 境界を確認する |
| UC 別 sequence 図 | [out/uc/](out/uc/) | 1 UC の画面、API、Entity 操作だけを確認する |
| API マトリクス | [out/api_matrix.csv](out/api_matrix.csv) | API と Entity CRUD の棚卸し |
| Event flow | [out/event_flow.mmd](out/event_flow.mmd) | BUC 間イベント連鎖を全体確認する |
| ER 図 | [out/er_care_to_billing.mmd](out/er_care_to_billing.mmd) | 診療から請求までのデータ関係を確認する |
| State 図 | [out/state_whole.mmd](out/state_whole.mmd) | 主要状態遷移の全体像を確認する |
| 状態到達表 | [out/states_appointment.txt](out/states_appointment.txt) | 予約状態の到達可能性を確認する |
| 状態到達表 | [out/states_claim.txt](out/states_claim.txt) | 請求状態の到達可能性を確認する |

生成コマンド例:

```sh
rdra-ish diagram samples/clinic-ops --kind rdra --format mermaid --buc BucClinicalEncounter --out samples/clinic-ops/out/buc/rdra_clinical_encounter
rdra-ish diagram samples/clinic-ops --kind sequence --format mermaid --buc BucClinicalEncounter --out samples/clinic-ops/out/buc/sequence_clinical_encounter
rdra-ish diagram samples/clinic-ops --kind sequence --format mermaid --usecase SignEncounter --out samples/clinic-ops/out/uc/sequence_sign_encounter
rdra-ish csv samples/clinic-ops --kind api-matrix --out samples/clinic-ops/out/api_matrix.csv
```

## 2. 業務スコープ

対象は診療所運営の中核業務です。患者登録、予約、受付、診療、検査、処方、請求、フォローアップ、運営管理を BUC として分け、業務担当者がレビューしやすい単位にしています。

| Business | BUC | 主な actor | 目的 |
|---|---|---|---|
| `PatientAccess` | `BucPatientOnboarding` | FrontDesk, Patient | 患者登録、保険確認、同意、問診送付 |
| `PatientAccess` | `BucAppointmentScheduling` | FrontDesk, Patient | 予約検索、予約、変更、取消、通知 |
| `PatientAccess` | `BucVisitCheckIn` | FrontDesk, Nurse | 来院確認、会計前受け、部屋割り、診療準備 |
| `CareDelivery` | `BucClinicalEncounter` | Nurse, Clinician | 診療記録、バイタル、診断、署名、予約完了 |
| `CareDelivery` | `BucOrdersResults` | Clinician, Nurse | 検査オーダー、検体、結果受領、結果確認 |
| `CareDelivery` | `BucPrescriptionFulfillment` | Clinician, Nurse | 処方作成、送信、調剤確認、取消、再処方 |
| `RevenueCycle` | `BucBillingClaims` | BillingSpecialist | チャージ、請求、入金、残高消込 |
| `PatientAccess` | `BucFollowupCare` | CareCoordinator, Patient | ケアプラン、患者フォロー、再診予約 |
| `ClinicAdministration` | `BucStaffAdministration` | ClinicAdmin | 予定枠、部屋、監査イベントの管理 |

レビューでは、まず BUC の責務境界を確認し、次に UC 単位で API/Entity 操作を確認します。最後に System/API マトリクスと ER/状態検証で横断的な整合性を見ます。

## 3. BUC 単位の設計レビュー

### 3.1 Patient Onboarding

| 項目 | 内容 |
|---|---|
| BUC | `BucPatientOnboarding` |
| Business | `PatientAccess` |
| 図 | [rdra_patient_onboarding.mmd](out/buc/rdra_patient_onboarding.mmd), [sequence_patient_onboarding.mmd](out/buc/sequence_patient_onboarding.mmd) |
| 主なデータ | `PatientAccount`, `PatientProfile`, `InsurancePolicy`, `EligibilityCheck`, `ConsentRecord`, `IntakePacket`, `PatientMessage` |

患者登録から問診送付までを扱います。患者マスタの新規作成、保険資格確認、同意取得、問診フォーム送付は同じ BUC にありますが、それぞれ API 境界を分けています。これにより、患者登録 API と外部資格確認 API の責務が混ざらないようにしています。

レビュー観点:

- `SearchPatient` と `RegisterPatient` の境界が明確か。
- `EligibilityApi` が保険確認結果と保険ポリシー更新を同じ整合性境界で扱ってよいか。
- `SendIntakeForms` は予約イベントから起動されるため、予約 BUC 側の責務と混同しないか。

### 3.2 Appointment Scheduling

| 項目 | 内容 |
|---|---|
| BUC | `BucAppointmentScheduling` |
| Business | `PatientAccess` |
| 図 | [rdra_appointment_scheduling.mmd](out/buc/rdra_appointment_scheduling.mmd), [sequence_appointment_scheduling.mmd](out/buc/sequence_appointment_scheduling.mmd) |
| 主なデータ | `Appointment`, `ProviderSchedule`, `Notification`, `PatientMessage` |

予約検索、仮押さえ、予約確定、変更、取消、通知を扱います。予約枠と予約レコードを同時に更新する UC は、それぞれ専用 API を持ちます。予約通知は `AppointmentNoticeApi` に分け、予約更新の整合性境界とは別にしています。

レビュー観点:

- `ReserveAppointment` と `BookAppointment` の差が業務上必要か。
- `CancelAppointment` が `Appointment` と `ProviderSchedule` を同じ API 境界で更新することが妥当か。
- `MarkNoShow` は現時点では direct CRUD だが、API 境界を追加すべきか。

### 3.3 Visit Check-In

| 項目 | 内容 |
|---|---|
| BUC | `BucVisitCheckIn` |
| Business | `PatientAccess` |
| 図 | [rdra_visit_check_in.mmd](out/buc/rdra_visit_check_in.mmd), [sequence_visit_check_in.mmd](out/buc/sequence_visit_check_in.mmd) |
| 主なデータ | `Appointment`, `PatientAccount`, `InsurancePolicy`, `PaymentTransaction`, `AccountBalance`, `Room`, `PatientProfile` |

来院確認から部屋割り、診療準備までを扱います。受付業務と看護師業務が同じ BUC に含まれるため、UC 単位の sequence で担当境界を確認します。

レビュー観点:

- `VerifyArrival` は read-only API として十分か。
- `CheckInPatient` は予約と患者アカウントを同時更新する必要があるか。
- `AssignRoom` は `Room` と `Appointment` を同じ API 境界で更新することでよいか。

### 3.4 Clinical Encounter

| 項目 | 内容 |
|---|---|
| BUC | `BucClinicalEncounter` |
| Business | `CareDelivery` |
| 図 | [rdra_clinical_encounter.mmd](out/buc/rdra_clinical_encounter.mmd), [sequence_clinical_encounter.mmd](out/buc/sequence_clinical_encounter.mmd) |
| 主なデータ | `Encounter`, `Appointment`, `VitalSign`, `Diagnosis`, `Room` |

診療記録を開き、バイタル、診断、署名、予約完了までを扱います。`EvEncounterSigned` は検査、請求、ケアプラン作成を起動する重要なイベントです。署名前後で編集可能範囲が変わるため、状態検証の中心になります。

レビュー観点:

- `SignEncounter` を後続 BUC の起点イベントとして扱ってよいか。
- `CompleteAppointment` は診療署名イベントから自動的に起動される業務か。
- `AmendEncounter` が署名済み記録の修正として十分な粒度か。

### 3.5 Orders and Results

| 項目 | 内容 |
|---|---|
| BUC | `BucOrdersResults` |
| Business | `CareDelivery` |
| 図 | [rdra_orders_results.mmd](out/buc/rdra_orders_results.mmd), [sequence_orders_results.mmd](out/buc/sequence_orders_results.mmd) |
| 主なデータ | `ClinicalOrder`, `LabResult`, `Notification`, `PatientMessage` |

検査オーダーから結果確認、重要結果通知までを扱います。検査システム連携は external system として扱い、内部 DSL では API と Entity の境界を確認します。

レビュー観点:

- `ReceiveLabResult` で `LabResult` 作成と `ClinicalOrder` 更新を同じ境界にしてよいか。
- `NotifyCriticalResult` は結果確認イベントから起動されるため、手動 UC と自動 UC の違いをレビューできるか。
- `CancelClinicalOrder` は direct CRUD のままでよいか。

### 3.6 Prescription Fulfillment

| 項目 | 内容 |
|---|---|
| BUC | `BucPrescriptionFulfillment` |
| Business | `CareDelivery` |
| 図 | [rdra_prescription_fulfillment.mmd](out/buc/rdra_prescription_fulfillment.mmd), [sequence_prescription_fulfillment.mmd](out/buc/sequence_prescription_fulfillment.mmd) |
| 主なデータ | `Medication`, `Encounter`, `Prescription` |

処方作成、薬剤検索、薬局ネットワーク送信、調剤確認、取消、再処方を扱います。外部薬局ネットワークは `uses` で表し、内部 API 境界は UC ごとに分けています。

レビュー観点:

- `DraftPrescription` と `SendPrescription` の間に承認 UC が必要か。
- `RefillPrescription` が新しい `Prescription` を作成する設計でよいか。
- `ConfirmDispense` は外部通知イベントとして扱うべきか。

### 3.7 Billing Claims

| 項目 | 内容 |
|---|---|
| BUC | `BucBillingClaims` |
| Business | `RevenueCycle` |
| 図 | [rdra_billing_claims.mmd](out/buc/rdra_billing_claims.mmd), [sequence_billing_claims.mmd](out/buc/sequence_billing_claims.mmd) |
| 主なデータ | `Charge`, `Claim`, `InsurancePolicy`, `PaymentTransaction`, `AccountBalance` |

診療署名後のチャージ作成、請求生成、請求送信、受理/否認、入金、残高消込を扱います。Revenue Cycle の状態検証では `Claim` が中心になります。

レビュー観点:

- `CreateCharge` が `AccountBalance` も更新する境界でよいか。
- `GenerateClaim` と `SubmitClaim` を分ける必要があるか。
- `PostPayment` と `ReconcileBalance` の責務境界が明確か。

### 3.8 Follow-Up Care

| 項目 | 内容 |
|---|---|
| BUC | `BucFollowupCare` |
| Business | `PatientAccess` |
| 図 | [rdra_followup_care.mmd](out/buc/rdra_followup_care.mmd), [sequence_followup_care.mmd](out/buc/sequence_followup_care.mmd) |
| 主なデータ | `CarePlan`, `Encounter`, `PatientMessage`, `Appointment`, `ProviderSchedule`, `Notification` |

診療後のケアプラン作成、患者連絡、患者応答確認、再診予約、ケアプラン終了を扱います。診療署名イベントと検査結果確認イベントを受けるため、イベントフロー上は複数 BUC と接続します。

レビュー観点:

- `CreateCarePlan` が `EvEncounterSigned` から起動される設計でよいか。
- `ScheduleFollowUpVisit` は予約 BUC に移すべきか、フォローアップ BUC に置くべきか。
- 患者応答を `PatientMessage` の状態として扱うだけで足りるか。

### 3.9 Staff Administration

| 項目 | 内容 |
|---|---|
| BUC | `BucStaffAdministration` |
| Business | `ClinicAdministration` |
| 図 | [rdra_staff_administration.mmd](out/buc/rdra_staff_administration.mmd), [sequence_staff_administration.mmd](out/buc/sequence_staff_administration.mmd) |
| 主なデータ | `ProviderSchedule`, `Provider`, `Room`, `AuditEvent`, `PatientAccount` |

診療所運営のマスタ・監査系業務を扱います。患者アクセスや診療とは別 Business に分けることで、通常業務と管理業務の責務を分けています。

レビュー観点:

- `ManageProviderSchedule` と `BlockScheduleSlot` の粒度が適切か。
- `ReviewAuditEvents` は read-only API として十分か。
- `ResolveAuditFinding` は direct CRUD のままでよいか。

## 4. UC 単位のレビュー

UC 単位の sequence は `out/uc/sequence_<usecase>.mmd` に生成しています。BUC 図で責務境界を見たあと、論点のある UC だけを個別に開きます。

### 4.1 Patient Onboarding UC

| UC | sequence | 主な確認点 |
|---|---|---|
| `SearchPatient` | [sequence_search_patient.mmd](out/uc/sequence_search_patient.mmd) | 患者検索が read-only であること |
| `RegisterPatient` | [sequence_register_patient.mmd](out/uc/sequence_register_patient.mmd) | `PatientAccount` と `PatientProfile` を同時作成すること |
| `UpdateDemographics` | [sequence_update_demographics.mmd](out/uc/sequence_update_demographics.mmd) | 患者属性更新の API 境界 |
| `VerifyInsurance` | [sequence_verify_insurance.mmd](out/uc/sequence_verify_insurance.mmd) | 保険参照、資格確認作成、保険状態更新 |
| `CaptureConsent` | [sequence_capture_consent.mmd](out/uc/sequence_capture_consent.mmd) | 同意記録作成と nullable/Bool effect |
| `SendIntakeForms` | [sequence_send_intake_forms.mmd](out/uc/sequence_send_intake_forms.mmd) | 問診パケットとメッセージ作成 |
| `CompleteIntakeForms` | [sequence_complete_intake_forms.mmd](out/uc/sequence_complete_intake_forms.mmd) | 問診完了イベント |
| `ExpireIntakeForms` | [sequence_expire_intake_forms.mmd](out/uc/sequence_expire_intake_forms.mmd) | 問診期限切れイベント |
| `MergeDuplicatePatient` | [sequence_merge_duplicate_patient.mmd](out/uc/sequence_merge_duplicate_patient.mmd) | 患者統合状態 |
| `ArchivePatient` | [sequence_archive_patient.mmd](out/uc/sequence_archive_patient.mmd) | 患者アーカイブ状態 |

### 4.2 Appointment Scheduling UC

| UC | sequence | 主な確認点 |
|---|---|---|
| `SearchAvailability` | [sequence_search_availability.mmd](out/uc/sequence_search_availability.mmd) | 予定枠検索の read-only 境界 |
| `ReserveAppointment` | [sequence_reserve_appointment.mmd](out/uc/sequence_reserve_appointment.mmd) | 予約作成と予定枠更新 |
| `BookAppointment` | [sequence_book_appointment.mmd](out/uc/sequence_book_appointment.mmd) | 予約確定と予定枠更新 |
| `RescheduleAppointment` | [sequence_reschedule_appointment.mmd](out/uc/sequence_reschedule_appointment.mmd) | 予約変更と予定枠更新 |
| `CancelAppointment` | [sequence_cancel_appointment.mmd](out/uc/sequence_cancel_appointment.mmd) | 予約取消と予定枠更新 |
| `MarkNoShow` | [sequence_mark_no_show.mmd](out/uc/sequence_mark_no_show.mmd) | no-show 状態への直接更新 |
| `SendAppointmentNotice` | [sequence_send_appointment_notice.mmd](out/uc/sequence_send_appointment_notice.mmd) | 通知と患者メッセージ作成 |

### 4.3 Visit Check-In UC

| UC | sequence | 主な確認点 |
|---|---|---|
| `VerifyArrival` | [sequence_verify_arrival.mmd](out/uc/sequence_verify_arrival.mmd) | 受付時の参照情報 |
| `CheckInPatient` | [sequence_check_in_patient.mmd](out/uc/sequence_check_in_patient.mmd) | 予約と患者アカウント更新 |
| `CollectCopay` | [sequence_collect_copay.mmd](out/uc/sequence_collect_copay.mmd) | 会計取引と残高更新 |
| `AssignRoom` | [sequence_assign_room.mmd](out/uc/sequence_assign_room.mmd) | 部屋と予約の同期更新 |
| `PrepareEncounter` | [sequence_prepare_encounter.mmd](out/uc/sequence_prepare_encounter.mmd) | 診療準備の参照情報 |

### 4.4 Clinical Encounter UC

| UC | sequence | 主な確認点 |
|---|---|---|
| `OpenEncounter` | [sequence_open_encounter.mmd](out/uc/sequence_open_encounter.mmd) | 予約から診療記録を開始する |
| `RecordVitals` | [sequence_record_vitals.mmd](out/uc/sequence_record_vitals.mmd) | バイタル作成と診療記録更新 |
| `DocumentAssessment` | [sequence_document_assessment.mmd](out/uc/sequence_document_assessment.mmd) | 診断作成と診療記録更新 |
| `SignEncounter` | [sequence_sign_encounter.mmd](out/uc/sequence_sign_encounter.mmd) | 診療署名イベント |
| `AmendEncounter` | [sequence_amend_encounter.mmd](out/uc/sequence_amend_encounter.mmd) | 署名後修正 |
| `CompleteAppointment` | [sequence_complete_appointment.mmd](out/uc/sequence_complete_appointment.mmd) | 予約完了と部屋清掃状態 |

### 4.5 Orders, Prescription, Billing, Follow-Up, Admin UC

| 領域 | 主な UC | レビュー観点 |
|---|---|---|
| Orders/Results | `PlaceLabOrder`, `CollectSpecimen`, `ReceiveLabResult`, `ReviewLabResult`, `NotifyCriticalResult`, `CancelClinicalOrder` | 検査オーダー状態と結果通知のイベント境界 |
| Prescription | `SearchMedication`, `DraftPrescription`, `SendPrescription`, `ConfirmDispense`, `CancelPrescription`, `RefillPrescription` | 外部薬局ネットワークと内部処方状態の境界 |
| Billing | `CreateCharge`, `GenerateClaim`, `SubmitClaim`, `ReceiveClaimAccepted`, `RecordClaimDenial`, `PostPayment`, `ReconcileBalance`, `VoidCharge` | Claim lifecycle と残高更新 |
| Follow-Up | `CreateCarePlan`, `SendFollowUpMessage`, `ReviewPatientResponse`, `ScheduleFollowUpVisit`, `CloseCarePlan`, `NotifyPatientResult` | 診療後イベントから患者フォローへ流れる責務 |
| Staff Admin | `ManageProviderSchedule`, `BlockScheduleSlot`, `ConfigureRoom`, `ReleaseRoom`, `ReviewAuditEvents`, `ResolveAuditFinding` | 管理業務を通常診療 BUC から分離できているか |

## 5. System/API 単位の設計

### 5.1 System 境界

本サンプルでは、内部 System を `ClinicOpsSystem` に集約しています。現時点の意図は、分散システム境界よりも API と Entity CRUD の棚卸しをレビューすることです。複数 System への分割は、外部サービスや所有組織が明確になった時点で追加する想定です。

```rdra
system ClinicOpsSystem "Clinic Operations System"
contains(ClinicOpsSystem, <EachApi>)
```

### 5.2 API 設計方針

API は原則として UC ごとの一貫性境界に分けています。1 つの API を複数 UC から共有すると、sequence 図ではその API の CRUD 全体が各 UC に適用されるため、意図しない Entity 操作が混ざります。

| 方針 | 理由 |
|---|---|
| 書き込み UC は原則 UC 専用 API を持つ | sequence 図で責務境界が混ざらないようにする |
| read-only UC も必要に応じて API 化する | API matrix で参照責務を確認する |
| direct CRUD は単純な状態更新に限定する | API 境界を置くほどの一貫性責務がない場合のみ使用する |

### 5.3 API マトリクスの読み方

[api_matrix.csv](out/api_matrix.csv) は、API を行、Entity を列にした CRUD 表です。レビューでは次を確認します。

- 1 API が不自然に多くの Entity を操作していないか。
- read-only API と write API が混ざっていないか。
- 複数 Entity 更新が、業務上 1 つの atomic boundary として説明できるか。
- direct CRUD のまま残っている UC が、将来 API 化すべき候補ではないか。

## 6. Event Flow 単位の設計

Event flow は BUC をまたぐ連鎖だけを見るための図です。sequence 図には BUC 外の triggered UC を混ぜません。これはレビュー粒度を分けるためです。

重要なイベント:

| Event | 発生元 | 後続 |
|---|---|---|
| `EvAppointmentScheduled` | `BookAppointment`, `ScheduleFollowUpVisit` | `SendAppointmentNotice`, `SendIntakeForms` |
| `EvAppointmentCheckedIn` | `CheckInPatient` | `OpenEncounter`, `PrepareEncounter` |
| `EvEncounterSigned` | `SignEncounter` | `CompleteAppointment`, `PlaceLabOrder`, `CreateCharge`, `CreateCarePlan` |
| `EvClinicalOrderReviewed` | `ReviewLabResult` | `NotifyCriticalResult`, `NotifyPatientResult` |
| `EvClaimSubmitted` | `SubmitClaim` | Claim state transition |
| `EvClaimPaid` | `PostPayment` | Claim state transition |

Event flow のレビューでは「後続 BUC が起動するか」を確認し、sequence のレビューでは「対象 BUC/UC の中でどの API と Entity が動くか」を確認します。

## 7. 最終データモデリング

### 7.1 データ領域

| 領域 | Entity |
|---|---|
| 患者マスタ | `PatientAccount`, `PatientProfile`, `InsurancePolicy`, `EligibilityCheck`, `ConsentRecord`, `IntakePacket` |
| 施設・予定 | `Provider`, `ClinicLocation`, `Room`, `ProviderSchedule` |
| 予約・診療 | `Appointment`, `Encounter`, `VitalSign`, `Diagnosis` |
| オーダー・結果・処方 | `ClinicalOrder`, `LabResult`, `Medication`, `Prescription` |
| 請求 | `Charge`, `Claim`, `PaymentTransaction`, `AccountBalance` |
| フォロー・通知・監査 | `CarePlan`, `PatientMessage`, `Notification`, `AuditEvent` |

### 7.2 主要リレーション

| 関係 | 意味 |
|---|---|
| `PatientProfile -> PatientAccount` | 患者基本情報は患者アカウントに属する |
| `InsurancePolicy -> PatientAccount` | 保険ポリシーは患者ごとに管理する |
| `Appointment -> PatientAccount` | 予約は患者に紐づく |
| `Encounter -> Appointment` | 診療記録は予約から開始される |
| `VitalSign -> Encounter` | バイタルは診療記録に属する |
| `Diagnosis -> Encounter` | 診断は診療記録に属する |
| `ClinicalOrder -> Encounter` | 検査等のオーダーは診療記録から発生する |
| `LabResult -> ClinicalOrder` | 検査結果はオーダーに対応する |
| `Prescription -> Encounter` | 処方は診療記録に属する |
| `Charge -> Encounter` | チャージは診療記録に基づく |
| `Claim -> PatientAccount` | 請求は患者に紐づく |
| `PaymentTransaction -> Claim` | 入金は請求に紐づく |
| `CarePlan -> Encounter` | ケアプランは診療記録から作成される |

### 7.3 状態検証

状態検証は `Appointment` と `Claim` を代表として出力しています。

| Entity | 成果物 | 確認点 |
|---|---|---|
| `Appointment` | [states_appointment.txt](out/states_appointment.txt) | scheduled/check-in/completed/cancelled/no-show の到達可能性 |
| `Claim` | [states_claim.txt](out/states_claim.txt) | draft/submitted/accepted/denied/paid の到達可能性 |

状態検証で見るべきこと:

- 初期状態から想定する終端状態へ到達できるか。
- 到達してはいけない状態組み合わせがないか。
- nullable column や Bool column の `sets` が不足していないか。

## 8. レビュー手順

1. BUC 単位の RDRA 図でスコープと actor/usecase を確認する。
2. BUC 単位の sequence 図で、その BUC 内の書き込み系 UC と API 境界を確認する。
3. 論点のある UC は UC 単位 sequence 図で個別に確認する。
4. Event flow で BUC 間連鎖を確認する。
5. API matrix で API/Entity CRUD の責務を横断確認する。
6. ER 図で最終データ構造を確認する。
7. State 図と状態到達表で lifecycle と rule の不足を確認する。

## 9. 承認条件

| 観点 | 承認条件 |
|---|---|
| BUC | 業務スコープが 9 BUC に分解され、担当 actor が説明できる |
| UC | 各 UC の主要 CRUD とイベントが説明できる |
| API | API matrix 上で責務の広すぎる API がない |
| Sequence | BUC/UC sequence に対象外の API が出ない |
| Event | BUC 間イベント連鎖が Event flow で説明できる |
| Data | ER と状態到達表で主要 lifecycle が確認できる |

## Summary

<!-- derived-from #3-buc-単位の設計レビュー -->
<!-- derived-from #4-uc-単位のレビュー -->
<!-- derived-from #5-systemapi-単位の設計 -->
<!-- derived-from #7-最終データモデリング -->

Clinic Ops の設計レビューは、巨大な全体図ではなく、BUC、UC、System/API、データモデルの順に分割して進める。sequence 図は BUC/UC の対象内に閉じ、BUC をまたぐ連鎖は Event flow で別に確認する。
