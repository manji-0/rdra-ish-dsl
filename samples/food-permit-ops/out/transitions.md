| entity | column | from | to | usecase | guards | actors | site |
| --- | --- | --- | --- | --- | --- | --- | --- |
| BusinessNotification | status | 届出内容確認待 (submitted) | 受付済 (accepted) | 営業届の受付 (AcceptNotification) |  | 保健所 許可担当職員 | samples/food-permit-ops/buc/buc_business_notification.rdra:66 |
| BusinessNotification | status | 受付済 (accepted) | 廃業 (closed) | 届出営業の廃業届 (FileNotificationClosure) |  | 営業者 | samples/food-permit-ops/buc/buc_business_notification.rdra:72 |
| BusinessPermit | status | 有効 (valid) | 有効 (valid) | 有効期間の更新 (ExtendPermitTerm) |  | 保健所 許可担当職員 | samples/food-permit-ops/buc/buc_renewal.rdra:93 |
| BusinessPermit | status | 有効 (valid) | 営業停止中 (suspended) | 許可台帳への営業停止の記録 (SuspendBusiness) |  | 保健所 許可担当職員 | samples/food-permit-ops/buc/buc_disposition.rdra:200 |
| BusinessPermit | status | 有効 (valid) | 失効（有効期間満了） (expired) | 有効期間の満了による失効 (ExpirePermit) |  |  | samples/food-permit-ops/buc/buc_renewal.rdra:97 |
| BusinessPermit | status | 有効 (valid) | 取消 (revoked) | 許可台帳への取消の記録 (RevokePermit) |  | 保健所 許可担当職員 | samples/food-permit-ops/buc/buc_disposition.rdra:193 |
| BusinessPermit | status | 有効 (valid) | 廃業 (closed) | 廃業届の提出 (FileClosureNotice) |  | 営業者 | samples/food-permit-ops/buc/buc_permit_changes.rdra:107 |
| BusinessPermit | status | 営業停止中 (suspended) | 有効 (valid) | 営業停止期間の満了 (LiftSuspension) |  |  | samples/food-permit-ops/buc/buc_disposition.rdra:221 |
| BusinessPermit | status | 営業停止中 (suspended) | 失効（有効期間満了） (expired) | 有効期間の満了による失効 (ExpirePermit) |  |  | samples/food-permit-ops/buc/buc_renewal.rdra:98 |
| BusinessPermit | status | 営業停止中 (suspended) | 取消 (revoked) | 許可台帳への取消の記録 (RevokePermit) |  | 保健所 許可担当職員 | samples/food-permit-ops/buc/buc_disposition.rdra:194 |
| BusinessPermit | status | 営業停止中 (suspended) | 廃業 (closed) | 廃業届の提出 (FileClosureNotice) |  | 営業者 | samples/food-permit-ops/buc/buc_permit_changes.rdra:108 |
| ChangeNotice | status | 受付 (received) | 受理 (accepted) | 変更届の確認 (ReviewChangeNotice) |  | 保健所 許可担当職員 | samples/food-permit-ops/buc/buc_permit_changes.rdra:89 |
| ChangeNotice | status | 受付 (received) | 新規申請が必要 (new_application_required) | 変更届の確認 (ReviewChangeNotice) |  | 保健所 許可担当職員 | samples/food-permit-ops/buc/buc_permit_changes.rdra:90 |
| Consultation | status | 相談中 (open) | 許可が必要 (permit_required) | 営業の種類の判定 (ClassifyBusiness) | 許可業種（施行令35条）である | 食品衛生監視員 | samples/food-permit-ops/buc/buc_consultation.rdra:64 |
| Consultation | status | 相談中 (open) | 届出が必要 (notification_required) | 営業の種類の判定 (ClassifyBusiness) | 届出業種である | 食品衛生監視員 | samples/food-permit-ops/buc/buc_consultation.rdra:65 |
| Consultation | status | 相談中 (open) | 許可・届出とも不要 (exempt) | 営業の種類の判定 (ClassifyBusiness) | 届出も要らない営業（施行令35条の2）である | 食品衛生監視員 | samples/food-permit-ops/buc/buc_consultation.rdra:66 |
| Disposition | status | 処分案 (proposed) | 意見陳述の通知済 (notified) | 聴聞・弁明の通知 (NotifyOpinionProcedure) | 意見陳述の手続を執る処分である | 保健所 許可担当職員 | samples/food-permit-ops/buc/buc_disposition.rdra:139 |
| Disposition | status | 処分案 (proposed) | 処分決定 (decided) | 施設の整備改善命令 (OrderImprovement) | 施設の整備改善命令である\|意見陳述を省く処分である | 都道府県知事等（保健所設置市長・特別区長） | samples/food-permit-ops/buc/buc_disposition.rdra:208 |
| Disposition | status | 意見陳述の通知済 (notified) | 意見陳述の手続終了 (heard) | 聴聞の実施 (HoldHearing) | 聴聞を執る処分である | 保健所 許可担当職員 | samples/food-permit-ops/buc/buc_disposition.rdra:147 |
| Disposition | status | 意見陳述の通知済 (notified) | 意見陳述の手続終了 (heard) | 弁明書の受理 (ReceiveExplanation) | 弁明の機会を与える処分である | 保健所 許可担当職員 | samples/food-permit-ops/buc/buc_disposition.rdra:156 |
| Disposition | status | 意見陳述の手続終了 (heard) | 処分決定 (decided) | 許可の取消の決定 (DecideRevocation) | 聴聞を執る処分である\|許可の取消である | 都道府県知事等（保健所設置市長・特別区長） | samples/food-permit-ops/buc/buc_disposition.rdra:170 |
| Disposition | status | 意見陳述の手続終了 (heard) | 処分決定 (decided) | 営業停止の決定 (DecideSuspension) | 営業の停止である | 都道府県知事等（保健所設置市長・特別区長） | samples/food-permit-ops/buc/buc_disposition.rdra:181 |
| Disposition | status | 意見陳述の手続終了 (heard) | 処分決定 (decided) | 施設の整備改善命令 (OrderImprovement) | 施設の整備改善命令である | 都道府県知事等（保健所設置市長・特別区長） | samples/food-permit-ops/buc/buc_disposition.rdra:207 |
| Disposition | status | 意見陳述の手続終了 (heard) | 処分しない (dropped) | 処分を見送る (DropDisposition) |  | 都道府県知事等（保健所設置市長・特別区長） | samples/food-permit-ops/buc/buc_disposition.rdra:217 |
| Disposition | status | 処分決定 (decided) | 確定（審査請求期間の経過） (finalized) | 審査請求期間の経過 (FinalizeDisposition) |  |  | samples/food-permit-ops/buc/buc_disposition.rdra:226 |
| Inspection | status | 検査予定 (scheduled) | 適合 (passed) | 施設検査の実施 (ConductInspection) |  | 食品衛生監視員 | samples/food-permit-ops/buc/buc_inspection_decision.rdra:126 |
| Inspection | status | 検査予定 (scheduled) | 不適合（改善の指示） (failed) | 施設検査の実施 (ConductInspection) |  | 食品衛生監視員 | samples/food-permit-ops/buc/buc_inspection_decision.rdra:127 |
| Inspection | status | 不適合（改善の指示） (failed) | 検査予定 (scheduled) | 改善の報告と再検査の申込み (RequestReinspection) |  | 営業者 | samples/food-permit-ops/buc/buc_inspection_decision.rdra:132 |
| MonitoringPlan | status | 策定中 (drafted) | 公表済 (published) | 監視指導計画の公表 (PublishMonitoringPlan) |  | 保健所 許可担当職員 | samples/food-permit-ops/buc/buc_monitoring.rdra:76 |
| PermitApplication | status | 申請内容確認待 (submitted) | 差戻し・補正中 (returned) | 差戻し（補正の求め） (ReturnApplication) |  | 保健所 許可担当職員 | samples/food-permit-ops/buc/buc_permit_application.rdra:122 |
| PermitApplication | status | 申請内容確認待 (submitted) | 検査待 (awaiting_inspection) | 申請内容の確認と受理 (ReviewApplication) | 手数料が納められている | 保健所 許可担当職員 | samples/food-permit-ops/buc/buc_permit_application.rdra:114 |
| PermitApplication | status | 申請内容確認待 (submitted) | 取下げ (withdrawn) | 申請の取下げ (WithdrawApplication) | 窓口（紙）で出した申請である | 営業者 | samples/food-permit-ops/buc/buc_permit_application.rdra:136 |
| PermitApplication | status | 差戻し・補正中 (returned) | 申請内容確認待 (submitted) | 補正した申請の再提出 (ResubmitApplication) |  | 営業者 | samples/food-permit-ops/buc/buc_permit_application.rdra:130 |
| PermitApplication | status | 差戻し・補正中 (returned) | 不許可 (denied) | 不許可の決定 (DenyPermit) |  | 都道府県知事等（保健所設置市長・特別区長） | samples/food-permit-ops/buc/buc_inspection_decision.rdra:155 |
| PermitApplication | status | 差戻し・補正中 (returned) | 取下げ (withdrawn) | 申請の取下げ (WithdrawApplication) |  | 営業者 | samples/food-permit-ops/buc/buc_permit_application.rdra:135 |
| PermitApplication | status | 検査待 (awaiting_inspection) | 差戻し・補正中 (returned) | 差戻し（補正の求め） (ReturnApplication) |  | 保健所 許可担当職員 | samples/food-permit-ops/buc/buc_permit_application.rdra:123 |
| PermitApplication | status | 検査待 (awaiting_inspection) | 許可 (approved) | 許可の決定 (GrantPermit) | 施設検査で施設基準に適合した\|新規の申請である | 都道府県知事等（保健所設置市長・特別区長） | samples/food-permit-ops/buc/buc_inspection_decision.rdra:139 |
| PermitApplication | status | 検査待 (awaiting_inspection) | 許可 (approved) | 更新の許可の決定 (RenewPermit) | 継続（更新）の申請である\|施設検査で施設基準に適合した | 都道府県知事等（保健所設置市長・特別区長） | samples/food-permit-ops/buc/buc_renewal.rdra:79 |
| PermitApplication | status | 検査待 (awaiting_inspection) | 不許可 (denied) | 不許可の決定 (DenyPermit) |  | 都道府県知事等（保健所設置市長・特別区長） | samples/food-permit-ops/buc/buc_inspection_decision.rdra:154 |
| PermitApplication | status | 検査待 (awaiting_inspection) | 取下げ (withdrawn) | 申請の取下げ (WithdrawApplication) | 窓口（紙）で出した申請である | 営業者 | samples/food-permit-ops/buc/buc_permit_application.rdra:137 |
| SiteVisit | status | 予定 (planned) | 違反なし (compliant) | 違反なしの記録 (RecordCompliance) |  | 食品衛生監視員 | samples/food-permit-ops/buc/buc_monitoring.rdra:90 |
| SiteVisit | status | 予定 (planned) | 違反あり (violation) | 違反の記録 (RecordViolation) |  | 食品衛生監視員 | samples/food-permit-ops/buc/buc_monitoring.rdra:94 |
| SuccessionNotice | status | 受付 (received) | 受理 (accepted) | 地位承継届の受理 (AcceptSuccessionNotice) |  | 保健所 許可担当職員 | samples/food-permit-ops/buc/buc_permit_changes.rdra:102 |
