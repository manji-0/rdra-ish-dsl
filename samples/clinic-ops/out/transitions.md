| entity | column | from | to | usecase | guards | actors | site |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Appointment | status | 仮押さえ (requested) | 予約済 (scheduled) | 予約の確定 (BookAppointment) |  | 医事課 受付担当 | samples/clinic-ops/buc/buc_appointment_scheduling.rdra:82 |
| Appointment | status | 仮押さえ (requested) | 予約済 (scheduled) | 再診の予約 (ScheduleFollowUpVisit) |  | 患者支援・地域連携担当 | samples/clinic-ops/buc/buc_followup_care.rdra:72 |
| Appointment | status | 予約済 (scheduled) | 予約済 (scheduled) | 予約の変更 (RescheduleAppointment) |  | 医事課 受付担当 | samples/clinic-ops/buc/buc_appointment_scheduling.rdra:93 |
| Appointment | status | 予約済 (scheduled) | 受付済 (checked_in) | 受付（初診・再診） (CheckInPatient) |  | 医事課 受付担当 | samples/clinic-ops/buc/buc_visit_checkin.rdra:97 |
| Appointment | status | 予約済 (scheduled) | 取消 (cancelled) | 予約の取消 (CancelAppointment) |  | 医事課 受付担当 | samples/clinic-ops/buc/buc_appointment_scheduling.rdra:104 |
| Appointment | status | 予約済 (scheduled) | 未来院 (no_show) | 未来院の処理 (MarkNoShow) |  | 医事課 受付担当 | samples/clinic-ops/buc/buc_appointment_scheduling.rdra:113 |
| Appointment | status | 受付済 (checked_in) | 受付済 (checked_in) | 診察室への案内 (AssignRoom) |  | 看護師 | samples/clinic-ops/buc/buc_visit_checkin.rdra:157 |
| Appointment | status | 受付済 (checked_in) | 診察中 (in_consult) | 診療記録を開く (OpenEncounter) |  | 看護師 | samples/clinic-ops/buc/buc_clinical_encounter.rdra:65 |
| Appointment | status | 診察中 (in_consult) | 計算待ち (awaiting_calc) | 診察終了 (CompleteConsultation) |  | 医師 | samples/clinic-ops/buc/buc_clinical_encounter.rdra:104 |
| Appointment | status | 計算待ち (awaiting_calc) | 会計待ち (awaiting_pay) | 会計計算入力 (CalculateVisitBill) |  | 医事課 会計担当 | samples/clinic-ops/buc/buc_visit_payment.rdra:65 |
| Appointment | status | 会計待ち (awaiting_pay) | 精算済 (settled) | 自動精算機での精算 (SettleAtKiosk) |  |  | samples/clinic-ops/buc/buc_visit_payment.rdra:110 |
| Appointment | status | 会計待ち (awaiting_pay) | 精算済 (settled) | 会計窓口での精算 (SettleAtWindow) |  | 医事課 会計担当 | samples/clinic-ops/buc/buc_visit_payment.rdra:104 |
| Appointment | status | 精算済 (settled) | 完了 (completed) | 領収証・明細書の交付 (IssueReceipt) |  | 医事課 会計担当 | samples/clinic-ops/buc/buc_visit_payment.rdra:123 |
| CarePlan | status | 作成済 (open) | 経過観察中 (monitoring) | 患者応答の確認 (ReviewPatientResponse) |  | 患者支援・地域連携担当 | samples/clinic-ops/buc/buc_followup_care.rdra:63 |
| CarePlan | status | 経過観察中 (monitoring) | 終了 (closed) | ケアプランの終了 (CloseCarePlan) |  | 患者支援・地域連携担当 | samples/clinic-ops/buc/buc_followup_care.rdra:76 |
| ClaimStatement | status | 作成中 (drafting) | 院内点検中 (inspecting) | レセプトの院内点検 (InspectClaimStatement) |  | 医事課 レセプト担当 | samples/clinic-ops/buc/buc_billing_claims.rdra:118 |
| ClaimStatement | status | 院内点検中 (inspecting) | 保留 (on_hold) | レセプトの保留（月遅れ） (HoldClaimStatement) |  | 医事課 レセプト担当 | samples/clinic-ops/buc/buc_billing_claims.rdra:123 |
| ClaimStatement | status | 院内点検中 (inspecting) | 請求済 (submitted) | レセプト請求 (SubmitClaimStatements) |  | 医事課 レセプト担当 | samples/clinic-ops/buc/buc_billing_claims.rdra:135 |
| ClaimStatement | status | 保留 (on_hold) | 院内点検中 (inspecting) | 保留レセプトの再提出準備 (ReleaseHeldStatement) |  | 医事課 レセプト担当 | samples/clinic-ops/buc/buc_billing_claims.rdra:127 |
| ClaimStatement | status | 請求済 (submitted) | 受付不能・要確認 (receipt_error) | 受付不能・要確認の確認 (RecordReceiptError) |  | 医事課 レセプト担当 | samples/clinic-ops/buc/buc_billing_claims.rdra:144 |
| ClaimStatement | status | 請求済 (submitted) | 審査中 (under_review) | レセプト受付（審査へ） (ReceiveClaimAccepted) |  |  | samples/clinic-ops/buc/buc_billing_claims.rdra:155 |
| ClaimStatement | status | 受付不能・要確認 (receipt_error) | 請求済 (submitted) | エラー分レセプトの再送 (ResendErroredStatement) |  | 医事課 レセプト担当 | samples/clinic-ops/buc/buc_billing_claims.rdra:150 |
| ClaimStatement | status | 審査中 (under_review) | 支払済 (paid) | 診療報酬の入金 (PostPayment) |  | 医事課 レセプト担当 | samples/clinic-ops/buc/buc_billing_claims.rdra:208 |
| ClaimStatement | status | 審査中 (under_review) | 返戻 (returned) | 返戻レセプトの取込 (RecordClaimReturn) |  | 医事課 レセプト担当 | samples/clinic-ops/buc/buc_billing_claims.rdra:171 |
| ClaimStatement | status | 審査中 (under_review) | 査定 (assessed) | 査定の取込 (RecordClaimAssessment) |  | 医事課 レセプト担当 | samples/clinic-ops/buc/buc_billing_claims.rdra:193 |
| ClaimStatement | status | 返戻 (returned) | 院内点検中 (inspecting) | 返戻レセプトの訂正（再請求） (CorrectReturnedStatement) |  | 医事課 レセプト担当 | samples/clinic-ops/buc/buc_billing_claims.rdra:186 |
| ClaimStatement | status | 査定 (assessed) | 支払済 (paid) | 診療報酬の入金 (PostPayment) |  | 医事課 レセプト担当 | samples/clinic-ops/buc/buc_billing_claims.rdra:209 |
| ClaimStatement | status | 査定 (assessed) | 再審査請求中 (rereview) | 再審査請求 (RequestReexamination) |  | 医事課 レセプト担当 | samples/clinic-ops/buc/buc_billing_claims.rdra:201 |
| ClaimStatement | status | 再審査請求中 (rereview) | 支払済 (paid) | 診療報酬の入金 (PostPayment) |  | 医事課 レセプト担当 | samples/clinic-ops/buc/buc_billing_claims.rdra:210 |
| ClinicalOrder | status | オーダ済 (ordered) | オーダ済 (ordered) | 代行入力オーダの確定 (ConfirmProxyOrder) |  | 医師 | samples/clinic-ops/buc/buc_orders_results.rdra:96 |
| ClinicalOrder | status | オーダ済 (ordered) | 採取済 (collected) | 採血 (CollectSpecimen) | 医師が確定したオーダ | 看護師 | samples/clinic-ops/buc/buc_orders_results.rdra:105 |
| ClinicalOrder | status | オーダ済 (ordered) | 取消 (cancelled) | オーダの取消 (CancelClinicalOrder) |  | 医師 | samples/clinic-ops/buc/buc_orders_results.rdra:154 |
| ClinicalOrder | status | 採取済 (collected) | 結果受信 (resulted) | 検査結果の受信 (ReceiveLabResult) |  | 看護師 | samples/clinic-ops/buc/buc_orders_results.rdra:128 |
| ClinicalOrder | status | 結果受信 (resulted) | 結果確認済 (reviewed) | 検査結果の確認 (ReviewLabResult) |  | 医師 | samples/clinic-ops/buc/buc_orders_results.rdra:140 |
| EligibilityCheck | result | 照会中 (pending) | 有効 (valid) | システム障害時モードでの資格確認 (ConfirmEligibilityInOutageMode) |  | 医事課 受付担当 | samples/clinic-ops/buc/buc_visit_checkin.rdra:144 |
| EligibilityCheck | result | 照会中 (pending) | 有効 (valid) | 資格確認結果（有効）の取込 (ReceiveEligibilityValid) |  |  | samples/clinic-ops/buc/buc_visit_checkin.rdra:116 |
| EligibilityCheck | result | 照会中 (pending) | 無効 (invalid) | 資格確認結果（無効）の取込 (ReceiveEligibilityInvalid) |  |  | samples/clinic-ops/buc/buc_visit_checkin.rdra:122 |
| EligibilityCheck | result | 照会中 (pending) | 該当資格なし (no_info) | 資格確認結果（該当資格なし）の取込 (ReceiveEligibilityNoInfo) |  |  | samples/clinic-ops/buc/buc_visit_checkin.rdra:127 |
| EligibilityCheck | result | 照会中 (pending) | 申立書で確認 (declared) | 被保険者資格申立書の受領 (RecordEligibilityDeclaration) |  | 医事課 受付担当 | samples/clinic-ops/buc/buc_visit_checkin.rdra:139 |
| EligibilityCheck | result | 無効 (invalid) | 照会中 (pending) | 資格の再確認 (RecheckEligibility) |  | 医事課 受付担当 | samples/clinic-ops/buc/buc_visit_checkin.rdra:132 |
| EligibilityCheck | result | 該当資格なし (no_info) | 照会中 (pending) | 資格の再確認 (RecheckEligibility) |  | 医事課 受付担当 | samples/clinic-ops/buc/buc_visit_checkin.rdra:133 |
| Encounter | status | 記録中 (open) | 記載済 (documented) | 診察・診断の記録 (DocumentAssessment) |  | 医師 | samples/clinic-ops/buc/buc_clinical_encounter.rdra:78 |
| Encounter | status | 記載済 (documented) | 署名済 (signed) | 診療記録の署名 (SignEncounter) |  | 医師 | samples/clinic-ops/buc/buc_clinical_encounter.rdra:84 |
| Encounter | status | 署名済 (signed) | 修正済 (amended) | 署名済み記録の修正 (AmendEncounter) |  | 医師 | samples/clinic-ops/buc/buc_clinical_encounter.rdra:90 |
| IntakePacket | status | 送付済 (sent) | 回答済 (completed) | 問診票の回答 (CompleteIntakeForms) |  | 患者 | samples/clinic-ops/buc/buc_patient_onboarding.rdra:117 |
| IntakePacket | status | 送付済 (sent) | 期限切れ (expired) | 問診票の期限切れ (ExpireIntakeForms) |  | 医事課 受付担当 | samples/clinic-ops/buc/buc_patient_onboarding.rdra:122 |
| PatientAccount | status | 有効 (active) | アーカイブ済 (inactive) | 患者のアーカイブ (ArchivePatient) |  | 医事課 受付担当 | samples/clinic-ops/buc/buc_patient_onboarding.rdra:131 |
| PatientAccount | status | 有効 (active) | 統合済 (merged) | 重複患者の統合 (MergeDuplicatePatient) |  | 医事課 受付担当 | samples/clinic-ops/buc/buc_patient_onboarding.rdra:127 |
| PaymentTransaction | status | 収納済 (posted) | 還付済 (refunded) | 還付・追加徴収 (RefundPayment) |  | 医事課 会計担当 | samples/clinic-ops/buc/buc_visit_payment.rdra:129 |
| Prescription | status | 処方入力中 (drafted) | 院外処方箋発行済 (issued) | 院外処方箋の発行 (IssueOutsidePrescription) |  | 医師 | samples/clinic-ops/buc/buc_prescription_fulfillment.rdra:104 |
| Prescription | status | 処方入力中 (drafted) | 引換券交付済 (voucher_issued) | 院内処方用引換券の交付 (IssueInhouseVoucher) |  | 医師 | samples/clinic-ops/buc/buc_prescription_fulfillment.rdra:110 |
| Prescription | status | 処方入力中 (drafted) | 取消 (cancelled) | 処方の取消 (CancelPrescription) |  | 医師 | samples/clinic-ops/buc/buc_prescription_fulfillment.rdra:163 |
| Prescription | status | 院外処方箋発行済 (issued) | 院外処方箋発行済 (issued) | 保険薬局からの疑義照会への回答 (AnswerPharmacyInquiry) |  | 医師 | samples/clinic-ops/buc/buc_prescription_fulfillment.rdra:142 |
| Prescription | status | 院外処方箋発行済 (issued) | 交付済 (dispensed) | 保険薬局での調剤結果の受信 (ConfirmDispense) |  |  | samples/clinic-ops/buc/buc_prescription_fulfillment.rdra:151 |
| Prescription | status | 院外処方箋発行済 (issued) | 使用期間切れ (expired) | 処方箋の使用期間切れ (ExpirePrescription) |  |  | samples/clinic-ops/buc/buc_prescription_fulfillment.rdra:158 |
| Prescription | status | 引換券交付済 (voucher_issued) | 疑義照会中 (in_inquiry) | 疑義照会 (RaisePrescriptionInquiry) |  | 薬剤師 | samples/clinic-ops/buc/buc_prescription_fulfillment.rdra:131 |
| Prescription | status | 引換券交付済 (voucher_issued) | 調剤・監査済 (audited) | 調剤・監査 (DispenseAndAudit) |  | 薬剤師 | samples/clinic-ops/buc/buc_prescription_fulfillment.rdra:115 |
| Prescription | status | 疑義照会中 (in_inquiry) | 引換券交付済 (voucher_issued) | 疑義照会への回答 (AnswerPrescriptionInquiry) |  | 医師 | samples/clinic-ops/buc/buc_prescription_fulfillment.rdra:136 |
| Prescription | status | 調剤・監査済 (audited) | 服薬指導済 (counseled) | 服薬指導 (CounselMedication) |  | 薬剤師 | samples/clinic-ops/buc/buc_prescription_fulfillment.rdra:119 |
| Prescription | status | 服薬指導済 (counseled) | 交付済 (dispensed) | 薬剤交付 (HandOverMedication) |  | 薬剤師 | samples/clinic-ops/buc/buc_prescription_fulfillment.rdra:123 |
