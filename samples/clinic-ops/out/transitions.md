| entity | column | from | to | usecase | guards | actors | site |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Appointment | status | 仮押さえ (apptrequested) | 予約済 (apptscheduled) | 予約の確定 (BookAppointment) |  | 医事課 受付担当 | samples/clinic-ops/buc/buc_appointment_scheduling.rdra:83 |
| Appointment | status | 仮押さえ (apptrequested) | 予約済 (apptscheduled) | 再診の予約 (ScheduleFollowUpVisit) |  | 患者支援・地域連携担当 | samples/clinic-ops/buc/buc_followup_care.rdra:71 |
| Appointment | status | 予約済 (apptscheduled) | 予約済 (apptscheduled) | 予約の変更 (RescheduleAppointment) |  | 医事課 受付担当 | samples/clinic-ops/buc/buc_appointment_scheduling.rdra:94 |
| Appointment | status | 予約済 (apptscheduled) | 受付済 (apptcheckedin) | 受付（初診・再診） (CheckInPatient) |  | 医事課 受付担当 | samples/clinic-ops/buc/buc_visit_checkin.rdra:96 |
| Appointment | status | 予約済 (apptscheduled) | 取消 (apptcancelled) | 予約の取消 (CancelAppointment) |  | 医事課 受付担当 | samples/clinic-ops/buc/buc_appointment_scheduling.rdra:105 |
| Appointment | status | 予約済 (apptscheduled) | 未来院 (apptnoshow) | 未来院の処理 (MarkNoShow) |  | 医事課 受付担当 | samples/clinic-ops/buc/buc_appointment_scheduling.rdra:114 |
| Appointment | status | 受付済 (apptcheckedin) | 受付済 (apptcheckedin) | 診察室への案内 (AssignRoom) |  | 看護師 | samples/clinic-ops/buc/buc_visit_checkin.rdra:156 |
| Appointment | status | 受付済 (apptcheckedin) | 診察中 (apptinconsult) | 診療記録を開く (OpenEncounter) |  | 看護師 | samples/clinic-ops/buc/buc_clinical_encounter.rdra:64 |
| Appointment | status | 診察中 (apptinconsult) | 計算待ち (apptawaitingcalc) | 診察終了 (CompleteConsultation) |  | 医師 | samples/clinic-ops/buc/buc_clinical_encounter.rdra:103 |
| Appointment | status | 計算待ち (apptawaitingcalc) | 会計待ち (apptawaitingpay) | 会計計算入力 (CalculateVisitBill) |  | 医事課 会計担当 | samples/clinic-ops/buc/buc_visit_payment.rdra:66 |
| Appointment | status | 会計待ち (apptawaitingpay) | 精算済 (apptsettled) | 自動精算機での精算 (SettleAtKiosk) |  |  | samples/clinic-ops/buc/buc_visit_payment.rdra:111 |
| Appointment | status | 会計待ち (apptawaitingpay) | 精算済 (apptsettled) | 会計窓口での精算 (SettleAtWindow) |  | 医事課 会計担当 | samples/clinic-ops/buc/buc_visit_payment.rdra:105 |
| Appointment | status | 精算済 (apptsettled) | 完了 (apptcompleted) | 領収証・明細書の交付 (IssueReceipt) |  | 医事課 会計担当 | samples/clinic-ops/buc/buc_visit_payment.rdra:124 |
| CarePlan | status | 作成済 (careopen) | 経過観察中 (caremonitoring) | 患者応答の確認 (ReviewPatientResponse) |  | 患者支援・地域連携担当 | samples/clinic-ops/buc/buc_followup_care.rdra:62 |
| CarePlan | status | 経過観察中 (caremonitoring) | 終了 (careclosed) | ケアプランの終了 (CloseCarePlan) |  | 患者支援・地域連携担当 | samples/clinic-ops/buc/buc_followup_care.rdra:75 |
| ClaimStatement | status | 作成中 (stmtdrafting) | 院内点検中 (stmtinspecting) | レセプトの院内点検 (InspectClaimStatement) |  | 医事課 レセプト担当 | samples/clinic-ops/buc/buc_billing_claims.rdra:119 |
| ClaimStatement | status | 院内点検中 (stmtinspecting) | 保留 (stmtonhold) | レセプトの保留（月遅れ） (HoldClaimStatement) |  | 医事課 レセプト担当 | samples/clinic-ops/buc/buc_billing_claims.rdra:124 |
| ClaimStatement | status | 院内点検中 (stmtinspecting) | 請求済 (stmtsubmitted) | レセプト請求 (SubmitClaimStatements) |  | 医事課 レセプト担当 | samples/clinic-ops/buc/buc_billing_claims.rdra:136 |
| ClaimStatement | status | 保留 (stmtonhold) | 院内点検中 (stmtinspecting) | 保留レセプトの再提出準備 (ReleaseHeldStatement) |  | 医事課 レセプト担当 | samples/clinic-ops/buc/buc_billing_claims.rdra:128 |
| ClaimStatement | status | 請求済 (stmtsubmitted) | 受付不能・要確認 (stmtreceipterror) | 受付不能・要確認の確認 (RecordReceiptError) |  | 医事課 レセプト担当 | samples/clinic-ops/buc/buc_billing_claims.rdra:145 |
| ClaimStatement | status | 請求済 (stmtsubmitted) | 審査中 (stmtunderreview) | レセプト受付（審査へ） (ReceiveClaimAccepted) |  |  | samples/clinic-ops/buc/buc_billing_claims.rdra:156 |
| ClaimStatement | status | 受付不能・要確認 (stmtreceipterror) | 請求済 (stmtsubmitted) | エラー分レセプトの再送 (ResendErroredStatement) |  | 医事課 レセプト担当 | samples/clinic-ops/buc/buc_billing_claims.rdra:151 |
| ClaimStatement | status | 審査中 (stmtunderreview) | 支払済 (stmtpaid) | 診療報酬の入金 (PostPayment) |  | 医事課 レセプト担当 | samples/clinic-ops/buc/buc_billing_claims.rdra:209 |
| ClaimStatement | status | 審査中 (stmtunderreview) | 返戻 (stmtreturned) | 返戻レセプトの取込 (RecordClaimReturn) |  | 医事課 レセプト担当 | samples/clinic-ops/buc/buc_billing_claims.rdra:172 |
| ClaimStatement | status | 審査中 (stmtunderreview) | 査定 (stmtassessed) | 査定の取込 (RecordClaimAssessment) |  | 医事課 レセプト担当 | samples/clinic-ops/buc/buc_billing_claims.rdra:194 |
| ClaimStatement | status | 返戻 (stmtreturned) | 院内点検中 (stmtinspecting) | 返戻レセプトの訂正（再請求） (CorrectReturnedStatement) |  | 医事課 レセプト担当 | samples/clinic-ops/buc/buc_billing_claims.rdra:187 |
| ClaimStatement | status | 査定 (stmtassessed) | 支払済 (stmtpaid) | 診療報酬の入金 (PostPayment) |  | 医事課 レセプト担当 | samples/clinic-ops/buc/buc_billing_claims.rdra:210 |
| ClaimStatement | status | 査定 (stmtassessed) | 再審査請求中 (stmtrereview) | 再審査請求 (RequestReexamination) |  | 医事課 レセプト担当 | samples/clinic-ops/buc/buc_billing_claims.rdra:202 |
| ClaimStatement | status | 再審査請求中 (stmtrereview) | 支払済 (stmtpaid) | 診療報酬の入金 (PostPayment) |  | 医事課 レセプト担当 | samples/clinic-ops/buc/buc_billing_claims.rdra:211 |
| ClinicalOrder | status | オーダ済 (clordered) | オーダ済 (clordered) | 代行入力オーダの確定 (ConfirmProxyOrder) |  | 医師 | samples/clinic-ops/buc/buc_orders_results.rdra:95 |
| ClinicalOrder | status | オーダ済 (clordered) | 採取済 (clcollected) | 採血 (CollectSpecimen) | 医師が確定したオーダ | 看護師 | samples/clinic-ops/buc/buc_orders_results.rdra:104 |
| ClinicalOrder | status | オーダ済 (clordered) | 取消 (clcancelled) | オーダの取消 (CancelClinicalOrder) |  | 医師 | samples/clinic-ops/buc/buc_orders_results.rdra:153 |
| ClinicalOrder | status | 採取済 (clcollected) | 結果受信 (clresulted) | 検査結果の受信 (ReceiveLabResult) |  | 看護師 | samples/clinic-ops/buc/buc_orders_results.rdra:127 |
| ClinicalOrder | status | 結果受信 (clresulted) | 結果確認済 (clreviewed) | 検査結果の確認 (ReviewLabResult) |  | 医師 | samples/clinic-ops/buc/buc_orders_results.rdra:139 |
| EligibilityCheck | result | 照会中 (eligpending) | 有効 (eligvalid) | システム障害時モードでの資格確認 (ConfirmEligibilityInOutageMode) |  | 医事課 受付担当 | samples/clinic-ops/buc/buc_visit_checkin.rdra:143 |
| EligibilityCheck | result | 照会中 (eligpending) | 有効 (eligvalid) | 資格確認結果（有効）の取込 (ReceiveEligibilityValid) |  |  | samples/clinic-ops/buc/buc_visit_checkin.rdra:115 |
| EligibilityCheck | result | 照会中 (eligpending) | 無効 (eliginvalid) | 資格確認結果（無効）の取込 (ReceiveEligibilityInvalid) |  |  | samples/clinic-ops/buc/buc_visit_checkin.rdra:121 |
| EligibilityCheck | result | 照会中 (eligpending) | 該当資格なし (elignoinfo) | 資格確認結果（該当資格なし）の取込 (ReceiveEligibilityNoInfo) |  |  | samples/clinic-ops/buc/buc_visit_checkin.rdra:126 |
| EligibilityCheck | result | 照会中 (eligpending) | 申立書で確認 (eligdeclared) | 被保険者資格申立書の受領 (RecordEligibilityDeclaration) |  | 医事課 受付担当 | samples/clinic-ops/buc/buc_visit_checkin.rdra:138 |
| EligibilityCheck | result | 無効 (eliginvalid) | 照会中 (eligpending) | 資格の再確認 (RecheckEligibility) |  | 医事課 受付担当 | samples/clinic-ops/buc/buc_visit_checkin.rdra:131 |
| EligibilityCheck | result | 該当資格なし (elignoinfo) | 照会中 (eligpending) | 資格の再確認 (RecheckEligibility) |  | 医事課 受付担当 | samples/clinic-ops/buc/buc_visit_checkin.rdra:132 |
| Encounter | status | 記録中 (encopen) | 記載済 (encdocumented) | 診察・診断の記録 (DocumentAssessment) |  | 医師 | samples/clinic-ops/buc/buc_clinical_encounter.rdra:77 |
| Encounter | status | 記載済 (encdocumented) | 署名済 (encsigned) | 診療記録の署名 (SignEncounter) |  | 医師 | samples/clinic-ops/buc/buc_clinical_encounter.rdra:83 |
| Encounter | status | 署名済 (encsigned) | 修正済 (encamended) | 署名済み記録の修正 (AmendEncounter) |  | 医師 | samples/clinic-ops/buc/buc_clinical_encounter.rdra:89 |
| IntakePacket | status | 送付済 (intakesent) | 回答済 (intakecompleted) | 問診票の回答 (CompleteIntakeForms) |  | 患者 | samples/clinic-ops/buc/buc_patient_onboarding.rdra:113 |
| IntakePacket | status | 送付済 (intakesent) | 期限切れ (intakeexpired) | 問診票の期限切れ (ExpireIntakeForms) |  | 医事課 受付担当 | samples/clinic-ops/buc/buc_patient_onboarding.rdra:118 |
| PatientAccount | status | 有効 (patientactive) | アーカイブ済 (patientinactive) | 患者のアーカイブ (ArchivePatient) |  | 医事課 受付担当 | samples/clinic-ops/buc/buc_patient_onboarding.rdra:127 |
| PatientAccount | status | 有効 (patientactive) | 統合済 (patientmerged) | 重複患者の統合 (MergeDuplicatePatient) |  | 医事課 受付担当 | samples/clinic-ops/buc/buc_patient_onboarding.rdra:123 |
| PaymentTransaction | status | 収納済 (payposted) | 還付済 (payrefunded) | 還付・追加徴収 (RefundPayment) |  | 医事課 会計担当 | samples/clinic-ops/buc/buc_visit_payment.rdra:130 |
| Prescription | status | 処方入力中 (rxdrafted) | 院外処方箋発行済 (rxissued) | 院外処方箋の発行 (IssueOutsidePrescription) |  | 医師 | samples/clinic-ops/buc/buc_prescription_fulfillment.rdra:100 |
| Prescription | status | 処方入力中 (rxdrafted) | 引換券交付済 (rxvoucher) | 院内処方用引換券の交付 (IssueInhouseVoucher) |  | 医師 | samples/clinic-ops/buc/buc_prescription_fulfillment.rdra:106 |
| Prescription | status | 処方入力中 (rxdrafted) | 取消 (rxcancelled) | 処方の取消 (CancelPrescription) |  | 医師 | samples/clinic-ops/buc/buc_prescription_fulfillment.rdra:159 |
| Prescription | status | 院外処方箋発行済 (rxissued) | 院外処方箋発行済 (rxissued) | 保険薬局からの疑義照会への回答 (AnswerPharmacyInquiry) |  | 医師 | samples/clinic-ops/buc/buc_prescription_fulfillment.rdra:138 |
| Prescription | status | 院外処方箋発行済 (rxissued) | 交付済 (rxdispensed) | 保険薬局での調剤結果の受信 (ConfirmDispense) |  |  | samples/clinic-ops/buc/buc_prescription_fulfillment.rdra:147 |
| Prescription | status | 院外処方箋発行済 (rxissued) | 使用期間切れ (rxexpired) | 処方箋の使用期間切れ (ExpirePrescription) |  |  | samples/clinic-ops/buc/buc_prescription_fulfillment.rdra:154 |
| Prescription | status | 引換券交付済 (rxvoucher) | 疑義照会中 (rxinquiry) | 疑義照会 (RaisePrescriptionInquiry) |  | 薬剤師 | samples/clinic-ops/buc/buc_prescription_fulfillment.rdra:127 |
| Prescription | status | 引換券交付済 (rxvoucher) | 調剤・監査済 (rxaudited) | 調剤・監査 (DispenseAndAudit) |  | 薬剤師 | samples/clinic-ops/buc/buc_prescription_fulfillment.rdra:111 |
| Prescription | status | 疑義照会中 (rxinquiry) | 引換券交付済 (rxvoucher) | 疑義照会への回答 (AnswerPrescriptionInquiry) |  | 医師 | samples/clinic-ops/buc/buc_prescription_fulfillment.rdra:132 |
| Prescription | status | 調剤・監査済 (rxaudited) | 服薬指導済 (rxcounseled) | 服薬指導 (CounselMedication) |  | 薬剤師 | samples/clinic-ops/buc/buc_prescription_fulfillment.rdra:115 |
| Prescription | status | 服薬指導済 (rxcounseled) | 交付済 (rxdispensed) | 薬剤交付 (HandOverMedication) |  | 薬剤師 | samples/clinic-ops/buc/buc_prescription_fulfillment.rdra:119 |
