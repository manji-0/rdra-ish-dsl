| entity | column | from | to | usecase | guards | actors | site |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Claim | period | 未起算 (not_started) | 30日の期間中 (running) | 請求完了の確認 (CompleteClaim) |  | 損害サービス担当者 | samples/insurance-claim-ops/buc/buc_claim.rdra:109 |
| Claim | period | 未起算 (not_started) | 期限の管理を終了 (ended) | 請求権の時効による消滅 (TimeBarClaim) |  |  | samples/insurance-claim-ops/buc/buc_claim.rdra:135 |
| Claim | period | 未起算 (not_started) | 期限の管理を終了 (ended) | 請求の取下げ (WithdrawClaim) |  | 契約者・被保険者 | samples/insurance-claim-ops/buc/buc_claim.rdra:125 |
| Claim | period | 30日の期間中 (running) | 延長期間中 (extended) | 支払期限の延長と通知 (ExtendPaymentPeriod) | 請求完了の後、支払・不払を決める前である | 損害サービス担当者 | samples/insurance-claim-ops/buc/buc_payment.rdra:105 |
| Claim | period | 30日の期間中 (running) | 期限経過 (overdue) | 支払期限の経過 (MarkPaymentOverdue) |  |  | samples/insurance-claim-ops/buc/buc_payment.rdra:189 |
| Claim | period | 30日の期間中 (running) | 期限の管理を終了 (ended) | 不払いの通知 (NotifyDenial) | 不払いの理由を説明した | 損害サービス担当者 | samples/insurance-claim-ops/buc/buc_payment.rdra:155 |
| Claim | period | 30日の期間中 (running) | 期限の管理を終了 (ended) | 保険金の支払 (PayClaim) | 支払済みの案件は再度支払わない | 損害サービス担当者 | samples/insurance-claim-ops/buc/buc_payment.rdra:171 |
| Claim | period | 30日の期間中 (running) | 期限の管理を終了 (ended) | 請求の取下げ (WithdrawClaim) |  | 契約者・被保険者 | samples/insurance-claim-ops/buc/buc_claim.rdra:126 |
| Claim | period | 延長期間中 (extended) | 期限経過 (overdue) | 支払期限の経過 (MarkPaymentOverdue) |  |  | samples/insurance-claim-ops/buc/buc_payment.rdra:190 |
| Claim | period | 延長期間中 (extended) | 期限の管理を終了 (ended) | 不払いの通知 (NotifyDenial) | 不払いの理由を説明した | 損害サービス担当者 | samples/insurance-claim-ops/buc/buc_payment.rdra:156 |
| Claim | period | 延長期間中 (extended) | 期限の管理を終了 (ended) | 保険金の支払 (PayClaim) | 支払済みの案件は再度支払わない | 損害サービス担当者 | samples/insurance-claim-ops/buc/buc_payment.rdra:172 |
| Claim | period | 延長期間中 (extended) | 期限の管理を終了 (ended) | 請求の取下げ (WithdrawClaim) |  | 契約者・被保険者 | samples/insurance-claim-ops/buc/buc_claim.rdra:127 |
| Claim | period | 期限経過 (overdue) | 期限の管理を終了 (ended) | 不払いの通知 (NotifyDenial) | 不払いの理由を説明した | 損害サービス担当者 | samples/insurance-claim-ops/buc/buc_payment.rdra:157 |
| Claim | period | 期限経過 (overdue) | 期限の管理を終了 (ended) | 保険金の支払 (PayClaim) | 支払済みの案件は再度支払わない | 損害サービス担当者 | samples/insurance-claim-ops/buc/buc_payment.rdra:173 |
| Claim | period | 期限経過 (overdue) | 期限の管理を終了 (ended) | 請求の取下げ (WithdrawClaim) |  | 契約者・被保険者 | samples/insurance-claim-ops/buc/buc_claim.rdra:128 |
| Claim | status | 請求の案内済 (guided) | 請求書類の提出 (submitted) | 請求書類の提出 (SubmitClaim) | 等級の変動を説明して請求の意思を確かめた | 契約者・被保険者\|被害者（損害賠償請求権者） | samples/insurance-claim-ops/buc/buc_claim.rdra:98 |
| Claim | status | 請求の案内済 (guided) | 取下げ (withdrawn) | 請求の取下げ (WithdrawClaim) |  | 契約者・被保険者 | samples/insurance-claim-ops/buc/buc_claim.rdra:122 |
| Claim | status | 請求の案内済 (guided) | 時効消滅 (time_barred) | 請求権の時効による消滅 (TimeBarClaim) |  |  | samples/insurance-claim-ops/buc/buc_claim.rdra:133 |
| Claim | status | 請求書類の提出 (submitted) | 請求完了 (completed) | 請求完了の確認 (CompleteClaim) | 車両・人身傷害の事故の請求である | 損害サービス担当者 | samples/insurance-claim-ops/buc/buc_claim.rdra:107 |
| Claim | status | 請求書類の提出 (submitted) | 取下げ (withdrawn) | 請求の取下げ (WithdrawClaim) |  | 契約者・被保険者 | samples/insurance-claim-ops/buc/buc_claim.rdra:123 |
| Claim | status | 請求書類の提出 (submitted) | 時効消滅 (time_barred) | 請求権の時効による消滅 (TimeBarClaim) |  |  | samples/insurance-claim-ops/buc/buc_claim.rdra:134 |
| Claim | status | 請求完了 (completed) | 支払決定 (decided_payment) | 支払・不払の決裁 (DecideClaim) | 不正請求の疑いがない | 支払決裁者 | samples/insurance-claim-ops/buc/buc_payment.rdra:127 |
| Claim | status | 請求完了 (completed) | 支払決定 (decided_payment) | 疑義事案の支払・不払の決裁 (DecideSuspectedClaim) | 不正請求の疑いがある | 上位決裁者（不正請求対応） | samples/insurance-claim-ops/buc/buc_payment.rdra:136 |
| Claim | status | 請求完了 (completed) | 不払決定 (decided_denial) | 支払・不払の決裁 (DecideClaim) | 不正請求の疑いがない | 支払決裁者 | samples/insurance-claim-ops/buc/buc_payment.rdra:128 |
| Claim | status | 請求完了 (completed) | 不払決定 (decided_denial) | 疑義事案の支払・不払の決裁 (DecideSuspectedClaim) | 不正請求の疑いがある | 上位決裁者（不正請求対応） | samples/insurance-claim-ops/buc/buc_payment.rdra:137 |
| Claim | status | 請求完了 (completed) | 取下げ (withdrawn) | 請求の取下げ (WithdrawClaim) |  | 契約者・被保険者 | samples/insurance-claim-ops/buc/buc_claim.rdra:124 |
| Claim | status | 支払決定 (decided_payment) | 支払済 (paid) | 保険金の支払 (PayClaim) | 支払済みの案件は再度支払わない | 損害サービス担当者 | samples/insurance-claim-ops/buc/buc_payment.rdra:170 |
| Claim | status | 不払決定 (decided_denial) | 不払通知済 (denial_notified) | 不払いの通知 (NotifyDenial) | 不払いの理由を説明した | 損害サービス担当者 | samples/insurance-claim-ops/buc/buc_payment.rdra:154 |
| ClaimCase | liability | 未確定 (undetermined) | 賠償責任あり (at_fault) | 過失割合と賠償責任の判断 (DetermineFault) | 事故案件を調査しているか、損害を確定した | 損害サービス担当者 | samples/insurance-claim-ops/buc/buc_investigation.rdra:81 |
| ClaimCase | liability | 未確定 (undetermined) | 過失ゼロ（賠償責任なし） (no_fault) | 過失割合と賠償責任の判断 (DetermineFault) | 事故案件を調査しているか、損害を確定した | 損害サービス担当者 | samples/insurance-claim-ops/buc/buc_investigation.rdra:82 |
| ClaimCase | status | 事故受付 (received) | 初期対応済 (responded) | 初期対応（担当者の案内と手続の説明） (RespondInitially) |  | 損害サービス担当者 | samples/insurance-claim-ops/buc/buc_intake.rdra:71 |
| ClaimCase | status | 初期対応済 (responded) | 調査中 (investigating) | 損害調査の依頼 (RequestInvestigation) |  | 損害サービス担当者 | samples/insurance-claim-ops/buc/buc_investigation.rdra:68 |
| ClaimCase | status | 調査中 (investigating) | 示談交渉中 (negotiating) | 示談代行の開始 (OpenSettlement) | 対人・対物賠償の事故で、被保険者に賠償責任がある | 損害サービス担当者 | samples/insurance-claim-ops/buc/buc_settlement.rdra:80 |
| ClaimCase | status | 調査中 (investigating) | 損害確定 (assessed) | 損害の確定 (AssessDamage) | 損害調査の報告がある\|車両・人身傷害の事故である | 損害サービス担当者 | samples/insurance-claim-ops/buc/buc_investigation.rdra:96 |
| ClaimCase | status | 示談交渉中 (negotiating) | 損害確定 (assessed) | 損害の確定 (AssessDamage) | 損害調査の報告がある\|示談が成立したか、示談代行を終えた | 損害サービス担当者 | samples/insurance-claim-ops/buc/buc_investigation.rdra:98 |
| ClaimCase | status | 損害確定 (assessed) | 完了 (closed) | 事故案件の完了 (CloseCase) | 賠償責任の有無を判断している |  | samples/insurance-claim-ops/buc/buc_payment.rdra:197 |
| Complaint | status | 苦情受付 (received) | 解決 (resolved) | 苦情の解決 (ResolveComplaint) |  | 損害サービス担当者 | samples/insurance-claim-ops/buc/buc_complaint.rdra:90 |
| Complaint | status | 苦情受付 (received) | ADR 苦情解決手続中 (adr_complaint) | ADR 苦情の受付 (RecordAdrComplaint) |  |  | samples/insurance-claim-ops/buc/buc_complaint.rdra:104 |
| Complaint | status | 苦情受付 (received) | ADR 紛争解決手続中 (adr_dispute) | 紛争解決手続の申立ての受付 (RecordDisputeFiled) |  |  | samples/insurance-claim-ops/buc/buc_complaint.rdra:115 |
| Complaint | status | ADR 苦情解決手続中 (adr_complaint) | 解決 (resolved) | ADR 苦情への対応 (AnswerAdrComplaint) |  | 損害サービス担当者 | samples/insurance-claim-ops/buc/buc_complaint.rdra:110 |
| Complaint | status | ADR 苦情解決手続中 (adr_complaint) | ADR 紛争解決手続中 (adr_dispute) | 紛争解決手続の申立ての受付 (RecordDisputeFiled) |  |  | samples/insurance-claim-ops/buc/buc_complaint.rdra:116 |
| Complaint | status | ADR 紛争解決手続中 (adr_dispute) | 和解案の提示 (proposal_received) | 和解案の受領 (RecordProposal) |  |  | samples/insurance-claim-ops/buc/buc_complaint.rdra:121 |
| Complaint | status | ADR 紛争解決手続中 (adr_dispute) | 手続終了（不調） (terminated) | 紛争解決手続の終了 (RecordDisputeTerminated) |  |  | samples/insurance-claim-ops/buc/buc_complaint.rdra:143 |
| Complaint | status | 和解案の提示 (proposal_received) | 和解成立 (settled) | 和解案の受諾 (AcceptProposal) |  | 上位決裁者（不正請求対応） | samples/insurance-claim-ops/buc/buc_complaint.rdra:129 |
| Complaint | status | 和解案の提示 (proposal_received) | 和解案を受諾しない (rejected) | 和解案を受諾しない (RejectProposal) | 特別調停案ではない和解案である | 上位決裁者（不正請求対応） | samples/insurance-claim-ops/buc/buc_complaint.rdra:135 |
| Complaint | status | 和解案の提示 (proposal_received) | 手続終了（不調） (terminated) | 紛争解決手続の終了 (RecordDisputeTerminated) |  |  | samples/insurance-claim-ops/buc/buc_complaint.rdra:144 |
| Investigation | status | 調査依頼 (requested) | 調査中 (surveying) | 損害調査の実施 (ConductSurvey) |  | アジャスター | samples/insurance-claim-ops/buc/buc_investigation.rdra:73 |
| Investigation | status | 調査中 (surveying) | 調査報告済 (reported) | 損害調査の報告 (ReportSurvey) |  | アジャスター | samples/insurance-claim-ops/buc/buc_investigation.rdra:77 |
| JibaisekiSurvey | status | 調査事務所へ送付 (sent) | 地区本部・本部・審査会で審査中 (under_review) | 上部機関・審査会での審査の記録 (RecordJibaisekiEscalation) |  |  | samples/insurance-claim-ops/buc/buc_jibaiseki.rdra:61 |
| JibaisekiSurvey | status | 調査事務所へ送付 (sent) | 調査結果の報告受領 (reported) | 自賠責損害調査の結果の受領 (RecordJibaisekiResult) |  |  | samples/insurance-claim-ops/buc/buc_jibaiseki.rdra:66 |
| JibaisekiSurvey | status | 地区本部・本部・審査会で審査中 (under_review) | 調査結果の報告受領 (reported) | 自賠責損害調査の結果の受領 (RecordJibaisekiResult) |  |  | samples/insurance-claim-ops/buc/buc_jibaiseki.rdra:67 |
| Payment | status | 支払手続 (instructed) | 支払完了の連絡済 (completed) | 支払完了の連絡 (NotifyPaymentCompleted) |  | 損害サービス担当者 | samples/insurance-claim-ops/buc/buc_payment.rdra:184 |
| RepairEstimate | status | 見積提出 (submitted) | 修理費の協定 (agreed) | 修理費の協定 (AgreeRepairCost) |  | アジャスター | samples/insurance-claim-ops/buc/buc_repair.rdra:68 |
| RepairEstimate | status | 修理費の協定 (agreed) | 修理中 (repairing) | 修理の着工 (StartRepair) | 修理着工を承認した | 修理工場 | samples/insurance-claim-ops/buc/buc_repair.rdra:80 |
| RepairEstimate | status | 修理中 (repairing) | 修理完了 (completed) | 修理の完了 (CompleteRepair) |  | 修理工場 | samples/insurance-claim-ops/buc/buc_repair.rdra:88 |
| Settlement | status | 同意の確認中 (consent_pending) | 示談交渉中 (negotiating) | 示談交渉の開始 (BeginNegotiation) | 被保険者と被害者がともに同意している | 損害サービス担当者 | samples/insurance-claim-ops/buc/buc_settlement.rdra:104 |
| Settlement | status | 同意の確認中 (consent_pending) | 示談代行を行わない (declined) | 示談代行を行わない (DeclineSettlementService) |  | 損害サービス担当者 | samples/insurance-claim-ops/buc/buc_settlement.rdra:112 |
| Settlement | status | 示談交渉中 (negotiating) | 示談成立 (agreed) | 示談の成立 (AgreeSettlement) |  | 損害サービス担当者 | samples/insurance-claim-ops/buc/buc_settlement.rdra:117 |
| Settlement | status | 示談交渉中 (negotiating) | 不調（調停・訴訟へ） (failed) | 示談交渉の終了（不調） (EndNegotiationUnresolved) |  | 損害サービス担当者 | samples/insurance-claim-ops/buc/buc_settlement.rdra:122 |
| Subrogation | status | 求償中 (open) | 回収 (recovered) | 求償の回収 (RecoverSubrogation) |  | 損害サービス担当者 | samples/insurance-claim-ops/buc/buc_subrogation.rdra:65 |
| Subrogation | status | 求償中 (open) | 回収の断念 (abandoned) | 求償の断念 (AbandonSubrogation) |  | 損害サービス担当者 | samples/insurance-claim-ops/buc/buc_subrogation.rdra:69 |
| Subrogation | status | 求償中 (open) | 時効消滅 (time_barred) | 求償する債権の時効 (LapseSubrogation) |  |  | samples/insurance-claim-ops/buc/buc_subrogation.rdra:73 |
