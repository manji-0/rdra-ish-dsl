| entity | column | from | to | usecase | guards | actors | site |
| --- | --- | --- | --- | --- | --- | --- | --- |
| DraftingOrder | status | 依頼済 (requested) | 原稿受領 (drafted) | 出願書類案の納品 (SubmitDraft) |  | 弁理士（特許事務所） | samples/patent-ops/buc/buc_application_drafting.rdra:70 |
| DraftingOrder | status | 原稿受領 (drafted) | 修正依頼中 (revising) | 出願書類の修正依頼 (RequestDraftRevision) |  | 知的財産部 担当者 | samples/patent-ops/buc/buc_application_drafting.rdra:83 |
| DraftingOrder | status | 原稿受領 (drafted) | 出願承認済 (approved) | 出願書類の承認 (ApproveDraft) | 発明者が内容を確認している | 知的財産部 担当者 | samples/patent-ops/buc/buc_application_drafting.rdra:88 |
| DraftingOrder | status | 修正依頼中 (revising) | 原稿受領 (drafted) | 出願書類案の納品 (SubmitDraft) |  | 弁理士（特許事務所） | samples/patent-ops/buc/buc_application_drafting.rdra:71 |
| DraftingOrder | status | 出願承認済 (approved) | 出願済 (filed) | 出願番号・出願日の記録 (RecordFilingReceipt) |  | 知的財産部 担当者 | samples/patent-ops/buc/buc_filing_formality.rdra:90 |
| InventionDisclosure | status | 受付待ち (submitted) | 差戻し (returned) | 届出書の差戻し (ReturnDisclosure) |  | 知的財産部 担当者 | samples/patent-ops/buc/buc_invention_disclosure.rdra:69 |
| InventionDisclosure | status | 受付待ち (submitted) | 受付済 (accepted) | 届出の受付と付番 (AcceptDisclosure) |  | 知的財産部 担当者 | samples/patent-ops/buc/buc_invention_disclosure.rdra:65 |
| InventionDisclosure | status | 差戻し (returned) | 受付待ち (submitted) | 届出書の再提出 (ResubmitDisclosure) |  | 発明者 | samples/patent-ops/buc/buc_invention_disclosure.rdra:74 |
| InventionDisclosure | status | 受付済 (accepted) | 先行技術調査中 (searching) | 先行技術調査の依頼 (RequestPriorArtSearch) |  | 知的財産部 担当者 | samples/patent-ops/buc/buc_filing_decision.rdra:50 |
| InventionDisclosure | status | 先行技術調査中 (searching) | 調査済 (searched) | 先行技術調査の結果の登録 (RecordPriorArtSearch) |  | 知的財産部 担当者 | samples/patent-ops/buc/buc_filing_decision.rdra:54 |
| InventionDisclosure | status | 調査済 (searched) | 出願決定 (decided_file) | 出願可否の決定 (DecideFiling) | 会社が特許を受ける権利を取得している | 発明審査委員会 | samples/patent-ops/buc/buc_filing_decision.rdra:62 |
| InventionDisclosure | status | 調査済 (searched) | ノウハウとして秘匿 (decided_secret) | 出願可否の決定 (DecideFiling) |  | 発明審査委員会 | samples/patent-ops/buc/buc_filing_decision.rdra:63 |
| InventionDisclosure | status | 調査済 (searched) | 出願しない (decided_abandon) | 出願可否の決定 (DecideFiling) |  | 発明審査委員会 | samples/patent-ops/buc/buc_filing_decision.rdra:64 |
| InventorReward | status | 決定 (decided) | 通知済 (notified) | 発明者への通知 (NotifyReward) |  | 知的財産部 担当者 | samples/patent-ops/buc/buc_inventor_reward.rdra:61 |
| InventorReward | status | 通知済 (notified) | 意見の申出あり (objected) | 発明者の意見の申出 (RecordInventorOpinion) |  | 発明者 | samples/patent-ops/buc/buc_inventor_reward.rdra:66 |
| InventorReward | status | 通知済 (notified) | 支払済 (paid) | 相当の利益の支払 (PayReward) |  | 知的財産部 担当者 | samples/patent-ops/buc/buc_inventor_reward.rdra:74 |
| InventorReward | status | 意見の申出あり (objected) | 通知済 (notified) | 意見への回答 (AnswerInventorOpinion) |  | 知的財産部 担当者 | samples/patent-ops/buc/buc_inventor_reward.rdra:70 |
| OfficeAction | status | 応答期間中 (open) | 応答案作成済 (drafted) | 意見書・補正書案の作成 (DraftResponse) |  | 弁理士（特許事務所） | samples/patent-ops/buc/buc_office_action.rdra:140 |
| OfficeAction | status | 応答期間中 (open) | 期間経過（延長請求可） (overdue) | 応答期間の経過 (ExpireResponseTerm) |  |  | samples/patent-ops/buc/buc_office_action.rdra:160 |
| OfficeAction | status | 応答案作成済 (drafted) | 期間経過（延長請求可） (overdue) | 応答期間の経過 (ExpireResponseTerm) |  |  | samples/patent-ops/buc/buc_office_action.rdra:161 |
| OfficeAction | status | 応答案作成済 (drafted) | 応答済 (answered) | 意見書・補正書の提出 (FileResponse) |  | 弁理士（特許事務所） | samples/patent-ops/buc/buc_office_action.rdra:184 |
| OfficeAction | status | 期間経過（延長請求可） (overdue) | 応答期間中 (open) | 期間経過後の延長請求 (RequestLateExtension) | 期間内に延長していない | 弁理士（特許事務所） | samples/patent-ops/buc/buc_office_action.rdra:168 |
| OfficeAction | status | 期間経過（延長請求可） (overdue) | 応答なしで期間満了 (lapsed) | 応答なしでの期間満了 (LapseResponse) |  |  | samples/patent-ops/buc/buc_office_action.rdra:178 |
| OfficeAction | status | 応答済 (answered) | 応答期間中 (open) | 最後の拒絶理由通知の受領 (ReceiveFinalOfficeAction) |  | 弁理士（特許事務所） | samples/patent-ops/buc/buc_office_action.rdra:194 |
| OfficeAction | status | 応答済 (answered) | 案件終了 (closed) | 中間処理案件の終了 (CloseOfficeActionDocket) | 出願の審査が終わっている | 知的財産部 担当者 | samples/patent-ops/buc/buc_office_action.rdra:241 |
| Patent | status | 存続中 (in_force) | 追納期間中 (grace) | 納付期限の経過 (EnterGracePeriod) | 次の年が未納 |  | samples/patent-ops/buc/buc_annuity.rdra:113 |
| Patent | status | 存続中 (in_force) | 放棄 (abandoned) | 特許権の放棄 (AbandonPatent) |  | 知的財産部 担当者 | samples/patent-ops/buc/buc_annuity.rdra:150 |
| Patent | status | 存続中 (in_force) | 存続期間満了 (expired) | 存続期間の満了 (ExpirePatentTerm) |  |  | samples/patent-ops/buc/buc_annuity.rdra:155 |
| Patent | status | 追納期間中 (grace) | 存続中 (in_force) | 追納 (PayLateAnnuity) | 維持すると決めた権利である | 知的財産部 担当者 | samples/patent-ops/buc/buc_annuity.rdra:122 |
| Patent | status | 追納期間中 (grace) | 消滅（特許料の不納） (lapsed) | 追納期間の経過による消滅 (LapseUnpaidPatent) |  |  | samples/patent-ops/buc/buc_annuity.rdra:134 |
| Patent | status | 追納期間中 (grace) | 存続期間満了 (expired) | 存続期間の満了 (ExpirePatentTerm) |  |  | samples/patent-ops/buc/buc_annuity.rdra:156 |
| Patent | status | 消滅（特許料の不納） (lapsed) | 存続中 (in_force) | 特許権の回復 (RestorePatent) |  | 知的財産部 担当者 | samples/patent-ops/buc/buc_annuity.rdra:143 |
| PatentApplication | status | 出願済（方式審査） (filed) | 手続補正指令への対応中 (formality_order) | 手続補正指令書の受領 (ReceiveFormalityOrder) |  | 弁理士（特許事務所） | samples/patent-ops/buc/buc_filing_formality.rdra:99 |
| PatentApplication | status | 出願済（方式審査） (filed) | 審査請求済（審査中） (examining) | 出願審査の請求 (RequestExamination) |  | 弁理士（特許事務所） | samples/patent-ops/buc/buc_examination_request.rdra:66 |
| PatentApplication | status | 出願済（方式審査） (filed) | 取下げ擬制（審査請求なし） (deemed_withdrawn) | 審査請求期間の経過 (DeemWithdrawnNoRequest) |  |  | samples/patent-ops/buc/buc_examination_request.rdra:84 |
| PatentApplication | status | 出願済（方式審査） (filed) | 取下げ擬制（国内優先権の基礎） (priority_withdrawn) | 先の出願の取下げ擬制 (DeemPriorityBaseWithdrawn) | 国内優先権の基礎とされ、まだ係属している出願である |  | samples/patent-ops/buc/buc_filing_formality.rdra:157 |
| PatentApplication | status | 出願済（方式審査） (filed) | 取下げ・放棄 (withdrawn) | 出願の取下げ・放棄 (WithdrawApplication) |  | 弁理士（特許事務所） | samples/patent-ops/buc/buc_examination_request.rdra:97 |
| PatentApplication | status | 手続補正指令への対応中 (formality_order) | 出願済（方式審査） (filed) | 手続補正書の提出 (SubmitFormalityAmendment) |  | 弁理士（特許事務所） | samples/patent-ops/buc/buc_filing_formality.rdra:108 |
| PatentApplication | status | 手続補正指令への対応中 (formality_order) | 却下 (dismissed) | 出願の却下の記録 (RecordDismissal) |  | 知的財産部 担当者 | samples/patent-ops/buc/buc_filing_formality.rdra:118 |
| PatentApplication | status | 審査請求済（審査中） (examining) | 特許査定 (granted) | 特許査定の受領 (ReceiveDecisionToGrant) |  | 知的財産部 担当者 | samples/patent-ops/buc/buc_office_action.rdra:211 |
| PatentApplication | status | 審査請求済（審査中） (examining) | 拒絶査定 (rejected) | 拒絶査定の受領 (ReceiveDecisionOfRejection) |  | 知的財産部 担当者 | samples/patent-ops/buc/buc_office_action.rdra:218 |
| PatentApplication | status | 審査請求済（審査中） (examining) | 取下げ擬制（国内優先権の基礎） (priority_withdrawn) | 先の出願の取下げ擬制 (DeemPriorityBaseWithdrawn) | 国内優先権の基礎とされ、まだ係属している出願である |  | samples/patent-ops/buc/buc_filing_formality.rdra:158 |
| PatentApplication | status | 審査請求済（審査中） (examining) | 取下げ・放棄 (withdrawn) | 出願の取下げ・放棄 (WithdrawApplication) |  | 弁理士（特許事務所） | samples/patent-ops/buc/buc_examination_request.rdra:98 |
| PatentApplication | status | 特許査定 (granted) | 設定登録済 (registered) | 設定登録の記録 (RecordRegistration) | 第1〜3年分の特許料を納付している | 知的財産部 担当者 | samples/patent-ops/buc/buc_registration.rdra:80 |
| PatentApplication | status | 特許査定 (granted) | 却下 (dismissed) | 特許料の不納による却下の記録 (RecordRegistrationDismissal) | 第1〜3年分の特許料を納付していない | 知的財産部 担当者 | samples/patent-ops/buc/buc_registration.rdra:97 |
| PatentApplication | status | 拒絶査定 (rejected) | 拒絶査定の確定 (rejection_final) | 拒絶査定の確定 (FinalizeRejection) |  |  | samples/patent-ops/buc/buc_office_action.rdra:232 |
| PatentApplication | status | 拒絶査定 (rejected) | 拒絶査定不服審判を請求 (appealed) | 拒絶査定不服審判の請求 (RequestAppeal) |  | 弁理士（特許事務所） | samples/patent-ops/buc/buc_office_action.rdra:225 |
| PatentApplication | status | 取下げ擬制（審査請求なし） (deemed_withdrawn) | 審査請求済（審査中） (examining) | 審査請求の回復 (RestoreExaminationRequest) |  | 弁理士（特許事務所） | samples/patent-ops/buc/buc_examination_request.rdra:89 |
