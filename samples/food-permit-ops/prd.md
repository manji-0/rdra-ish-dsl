# Food Permit Ops PRD

<!-- derived-from ../../docs/language-reference.md -->
<!-- derived-from ../../docs/cli-reference.md -->

`scripts/gen-clinic-ops-docs.py` が `samples/food-permit-ops/` のモデルから CLI を通して生成する。図・表・説明文を手で直さず、`.rdra` を直して `scripts/check-sample-artifacts.sh` を実行する。題材は日本の保健所の食品営業許可業務（事前相談と営業の種類の判定、許可申請、施設検査、許可・不許可、有効期間と更新、変更・承継・廃業の届出、営業届出、監視指導と不利益処分）で、審査請求の中身・食品表示・輸入食品・食中毒調査・他法令の許可は対象外。業務規則は `.rdra` の `//` コメントに一次資料の出典（ファイル名:行）を付けている。

この文書はプロダクトマネージャー向けである。なぜ作るか、誰のためか、何を満たせばよいかを示す。画面・API・データの設計は [設計書](design-doc.md) にある。

## 1. 背景と目的

成功指標の現状値と目標値はモデルが置いた想定の数字で、一次資料はない。実際の保健所で使うときは、その自治体の実績で置き換える。

### 不利益処分を手続どおりに行う

取消は聴聞、停止と改善命令は弁明の機会を経て、理由を示して行う。手続を欠いた処分は取り消されうる。

| 指標 | 現状 | 目標 | 期間 |
|---|---|---|---|
| 意見陳述の手続または理由の提示を欠いた不利益処分の件数 | 年1件 | 年0件 | 年次 |

### 無許可営業をなくす

有効期間が満了した許可や取り消された許可で営業を続けると無許可営業になる。満了前の通知と更新、停止・取消の後の確認で、許可のない営業を残さない。

| 指標 | 現状 | 目標 | 期間 |
|---|---|---|---|
| 有効な許可のない営業を立入検査で見つけた件数 | 年12件 | 年0件 | 年次 |

### 開店予定日までに許可する

申請者は開店の2〜3週間前に申請する。標準処理期間のうちに施設検査と許可を終えないと、開店が遅れる。

| 指標 | 現状 | 目標 | 期間 |
|---|---|---|---|
| 標準処理期間（15開庁日）内に許可・不許可を決めた申請の割合 | 85% | 98% | 四半期 |

## 2. 対象ユーザー

| 利用者 | 役割 |
|---|---|
| 営業者 | 飲食店などの営業を営もうとする者と、許可・届出を受けて営業する者。事前相談をし、許可を申請し、手数料を納め、施設検査を受け、許可後は変更・承継・廃業を届け出る。 |
| 食品衛生監視員 | 事前相談で図面を見て営業の種類を判定し、許可前と更新前の施設検査、計画に沿った立入検査を行い、違反を見つけたら処分を起案する。 |
| 都道府県知事等（保健所設置市長・特別区長） | 許可・不許可と、整備改善命令・営業停止・許可取消の処分を決める行政庁。 |
| 保健所 許可担当職員 | 許可申請・届出の受付と審査、差戻し、許可台帳と許可証、聴聞・弁明の事務、処分の公表、監視指導計画の取りまとめを行う。 |

### 誰がどの業務に関わるか

`list --kind buc-roles --format markdown` の出力である。`担当` は利用者がその BUC の UC を行うこと、`イベント送信` と `イベント受信` は外部システムがその BUC とイベントをやり取りすることを示す。

| role | kind | 営業届出 | 変更届・地位の承継・廃業届 | 事前相談と営業の種類の判定 | 施設検査と許可・不許可 | 営業許可の申請と受付 | 有効期間の満了と更新 | 不利益処分と公表 | 監視指導計画と立入検査 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 営業者 | actor | 担当 | 担当 | 担当 | 担当 | 担当 | 担当 |  |  |
| 食品衛生監視員 | actor |  |  | 担当 | 担当 |  | 担当 | 担当 | 担当 |
| 都道府県知事等（保健所設置市長・特別区長） | actor |  |  |  | 担当 |  | 担当 | 担当 |  |
| 保健所 許可担当職員 | actor | 担当 | 担当 |  | 担当 | 担当 | 担当 | 担当 | 担当 |
| 食品衛生申請等システム（厚生労働省） | extsystem | イベント送信、イベント受信 |  |  | イベント受信 | イベント送信、イベント受信 | イベント受信 |  |  |

### システムコンテキスト図

```mermaid
%% rdra-ish diagram=context scope=whole model=samples/food-permit-ops
%% kinds: actor extsystem system
%% omitted: step permission usecase event
flowchart LR
  actor__BusinessOperator(["営業者"]):::actor
  %% desc actor__BusinessOperator: 飲食店などの営業を営もうとする者と、許可・届出を受けて営業する者。事前相談をし、許可を申請し、手数料を納め、施設検査を受け、許可後は変更・承継・廃業を届け出る。
  actor__FoodSanitationInspector(["食品衛生監視員"]):::actor
  %% desc actor__FoodSanitationInspector: 事前相談で図面を見て営業の種類を判定し、許可前と更新前の施設検査、計画に沿った立入検査を行い、違反を見つけたら処分を起案する。
  actor__PermitAuthority(["都道府県知事等（保健所設置市長・特別区長）"]):::actor
  %% desc actor__PermitAuthority: 許可・不許可と、整備改善命令・営業停止・許可取消の処分を決める行政庁。
  actor__PermitClerk(["保健所 許可担当職員"]):::actor
  %% desc actor__PermitClerk: 許可申請・届出の受付と審査、差戻し、許可台帳と許可証、聴聞・弁明の事務、処分の公表、監視指導計画の取りまとめを行う。
  extsystem__FoodSanitationSystem[["食品衛生申請等システム（厚生労働省）"]]:::extsystem
  system__PermitLedgerSystem["営業許可台帳システム（保健所）"]:::system
  actor__BusinessOperator -.->|営業届出、事前相談と営業の種類の判定、施設検査と許可・不許可、営業許可の申請と受付、変更届・地位の承継・廃業届、有効期間の満了と更新| system__PermitLedgerSystem
  %% derived actor__BusinessOperator -> system__PermitLedgerSystem via buc__BucBusinessNotification,buc__BucConsultation,buc__BucInspectionDecision,buc__BucPermitApplication,buc__BucPermitChanges,buc__BucRenewal
  actor__FoodSanitationInspector -.->|事前相談と営業の種類の判定、不利益処分と公表、施設検査と許可・不許可、有効期間の満了と更新、監視指導計画と立入検査| system__PermitLedgerSystem
  %% derived actor__FoodSanitationInspector -> system__PermitLedgerSystem via buc__BucConsultation,buc__BucDisposition,buc__BucInspectionDecision,buc__BucMonitoring,buc__BucRenewal
  actor__PermitAuthority -.->|不利益処分と公表、施設検査と許可・不許可、有効期間の満了と更新| system__PermitLedgerSystem
  %% derived actor__PermitAuthority -> system__PermitLedgerSystem via buc__BucDisposition,buc__BucInspectionDecision,buc__BucRenewal
  actor__PermitClerk -.->|営業届出、不利益処分と公表、施設検査と許可・不許可、有効期間の満了と更新、監視指導計画と立入検査、営業許可の申請と受付、変更届・地位の承継・廃業届| system__PermitLedgerSystem
  %% derived actor__PermitClerk -> system__PermitLedgerSystem via buc__BucBusinessNotification,buc__BucDisposition,buc__BucInspectionDecision,buc__BucMonitoring,buc__BucPermitApplication,buc__BucPermitChanges,buc__BucRenewal
  extsystem__FoodSanitationSystem -.->|電子届出の提出（整理番号 NIF）、電子申請の提出（整理番号 LIC）| system__PermitLedgerSystem
  %% derived extsystem__FoodSanitationSystem -> system__PermitLedgerSystem via event__EvOnlineApplicationSubmitted,event__EvOnlineNotificationSubmitted
  system__PermitLedgerSystem -.->|届出の受付（受付済）、許可の発行（発行済）、申請の差戻し、更新の許可| extsystem__FoodSanitationSystem
  %% derived system__PermitLedgerSystem -> extsystem__FoodSanitationSystem via event__EvApplicationReturned,event__EvNotificationAccepted,event__EvPermitIssued,event__EvRenewalGranted
  classDef actor fill:#ffe3e3,stroke:#c92a2a,color:#000;
  classDef extsystem fill:#e9ecef,stroke:#495057,color:#000;
  classDef system fill:#c5f6fa,stroke:#0b7285,color:#000;
```

## 3. 要求

要求を目的ごとにまとめる。「BUC」はその要求が動機付ける業務で、[5. ユーザージャーニー](#5-ユーザージャーニー) にその業務フローがある。次の図は目的、要求、BUC のつながりである。「やらないこと」の枠にある要求は [4. やらないこと](#4-やらないこと) に載せている。

```mermaid
%% rdra-ish diagram=requirement scope=whole model=samples/food-permit-ops
%% kinds: requirement goal buc
%% omitted: nfr quality constraint business flow usecase event system entity
flowchart LR
  requirement__ReqClassifyBeforeConstruction[/"工事の前に営業の種類を判定し、許可業種なら施設基準を説明する"\]:::requirement
  requirement__ReqDueProcessDisposition[/"不利益処分は処分基準に沿い、意見陳述の手続と理由の提示を経て行う"\]:::requirement
  requirement__ReqFeeNotRefunded[/"納めた手数料は、取下げ・不許可でも還付しない"\]:::requirement
  requirement__ReqHygieneManagerAfterPermit[/"申請時に食品衛生責任者が未定なら、許可後速やかに定めさせる"\]:::requirement
  requirement__ReqKeepLedgerCurrent[/"許可後の変更・承継・廃業を届出で台帳に反映する"\]:::requirement
  requirement__ReqNotificationWithoutStandard[/"届出は施設基準・施設検査・手数料・更新なしで受け付ける"\]:::requirement
  requirement__ReqPermitOnlyCompliantFacility[/"施設基準に適合した施設にだけ許可し、不許可には理由を示す"\]:::requirement
  requirement__ReqPlannedMonitoring[/"毎年度の計画を公表し、計画に沿って立入検査する"\]:::requirement
  requirement__ReqPublishViolations[/"処分を決めた違反を一定期間公表する"\]:::requirement
  requirement__ReqReceiveBothChannels[/"窓口と電子申請を同じ手順で受け付け、不備は補正を求める"\]:::requirement
  requirement__ReqRenewBeforeExpiry[/"満了の前に通知し、満了日までに更新を終える。失効した許可は新規扱いとする"\]:::requirement
  requirement__ReqStopOperationOnSanction[/"停止中と、取消・失効・廃業の後は営業させない"\]:::requirement
  goal__DueProcess[/"不利益処分を手続どおりに行う<br/>意見陳述の手続または理由の提示を欠いた不利益処分の件数: 年1件 → 年0件"/]:::goal
  %% desc goal__DueProcess: 取消は聴聞、停止と改善命令は弁明の機会を経て、理由を示して行う。手続を欠いた処分は取り消されうる。
  goal__NoUnlicensedOperation[/"無許可営業をなくす<br/>有効な許可のない営業を立入検査で見つけた件数: 年12件 → 年0件"/]:::goal
  %% desc goal__NoUnlicensedOperation: 有効期間が満了した許可や取り消された許可で営業を続けると無許可営業になる。満了前の通知と更新、停止・取消の後の確認で、許可のない営業を残さない。
  goal__TimelyDecision[/"開店予定日までに許可する<br/>標準処理期間（15開庁日）内に許可・不許可を決めた申請の割合: 85% → 98%"/]:::goal
  %% desc goal__TimelyDecision: 申請者は開店の2〜3週間前に申請する。標準処理期間のうちに施設検査と許可を終えないと、開店が遅れる。
  buc__BucBusinessNotification["営業届出"]:::buc
  %% desc buc__BucBusinessNotification: 許可業種でも届出不要の営業でもない営業は、あらかじめ営業所の名称・所在地などを届け出る。許可と違って施設基準はなく、施設検査も手数料も更新もない。届出営業者も食品衛生責任者を定め、廃業したら届け出る。
  buc__BucConsultation["事前相談と営業の種類の判定"]:::buc
  %% desc buc__BucConsultation: 営業者は施設の工事の前に図面を持って保健所に相談する。食品衛生監視員は営業の内容から、許可が要るか（施行令35条の32業種）、届出でよいか、どちらも要らないか（施行令35条の2）を判定し、許可業種なら施設基準と食品衛生責任者を説明する。
  buc__BucDisposition["不利益処分と公表"]:::buc
  %% desc buc__BucDisposition: 施設基準違反などで、行政庁は施設の整備改善を命じ、営業を停止し、許可を取り消す。取消の前には聴聞、ほかは弁明の機会を与える。ただし計測などで確かめた技術的基準の不充足に基準どおりにせよと命じる処分は、意見陳述を省ける。処分は理由を示して書面で行い、審査請求ができることを教える。違反者は公表する（練馬区は7日以上）。停止は定めた期間が過ぎると終わる。
  buc__BucInspectionDecision["施設検査と許可・不許可"]:::buc
  %% desc buc__BucInspectionDecision: 食品衛生監視員が施設を検査し、施設基準に合わなければ改善を指示して再検査する。行政庁は基準に合うと認めれば許可しなければならず、合わなければ理由を示して不許可にする。許可すると台帳に登録して許可証を交付し、営業者は許可証を掲示して営業を始める。食品衛生責任者が申請時に未定なら、許可後速やかに定める。
  buc__BucMonitoring["監視指導計画と立入検査"]:::buc
  %% desc buc__BucMonitoring: 都道府県知事等は毎年度、翌年度の監視指導計画を定めて公表し、計画に沿って食品衛生監視員に営業の場所を立入検査させる。違反を見つけたら、処分の起案に回す。
  buc__BucPermitApplication["営業許可の申請と受付"]:::buc
  %% desc buc__BucPermitApplication: 営業者は開店の2〜3週間前（東京都は工事完成の10日前ころ）に、申請書と図面を保健所の窓口に出すか、食品衛生申請等システムで電子申請し、条例の手数料を納める。保健所は記載事項と添付書類を確かめ、足りなければ補正を求め（差戻し）、整えば施設検査に進める（検査待）。申請者は差戻しの間に取り下げられる。
  buc__BucPermitChanges["変更届・地位の承継・廃業届"]:::buc
  %% desc buc__BucPermitChanges: 許可営業者は申請書の記載事項が変わったら速やかに（東京都は10日以内）変更届を出す。移転や営業者の交代は変更ではなく新規の申請になる。営業の譲渡・相続・合併・分割では許可営業者の地位が承継され、承継した者が遅滞なく届け出る。廃業したら許可証を添えて廃業届を出す。
  buc__BucRenewal["有効期間の満了と更新"]:::buc
  %% desc buc__BucRenewal: 許可には5年以上の有効期間が付く。満了の前に保健所が通知し（静岡市は2か月前）、営業者は満了前に継続（更新）の申請をする。更新でも施設検査があり、満了日までに許可を終える。更新しないまま満了すると許可は失効し、営業を続けると無許可営業になる。失効した許可は更新できず、新規の申請からやり直す。
  subgraph group_out_of_scope["やらないこと"]
    requirement__ReqNoAppealContents[/"審査請求と訴訟の中身は対象外とする"\]:::requirement
    requirement__ReqNoFoodLabeling[/"食品表示は対象外とする"\]:::requirement
    requirement__ReqNoFoodPoisoningInvestigation[/"食中毒の調査と、59条の廃棄命令は対象外とする"\]:::requirement
    requirement__ReqNoImportedFood[/"輸入食品の届出は対象外とする"\]:::requirement
    requirement__ReqNoOtherPermits[/"他法令の許可・届出は対象外とする"\]:::requirement
  end
  goal__DueProcess --> requirement__ReqDueProcessDisposition
  %% derived goal__DueProcess -> requirement__ReqDueProcessDisposition via requirement__ReqDueProcessDisposition
  goal__DueProcess --> requirement__ReqPublishViolations
  %% derived goal__DueProcess -> requirement__ReqPublishViolations via requirement__ReqPublishViolations
  goal__NoUnlicensedOperation --> requirement__ReqClassifyBeforeConstruction
  %% derived goal__NoUnlicensedOperation -> requirement__ReqClassifyBeforeConstruction via requirement__ReqClassifyBeforeConstruction
  goal__NoUnlicensedOperation --> requirement__ReqHygieneManagerAfterPermit
  %% derived goal__NoUnlicensedOperation -> requirement__ReqHygieneManagerAfterPermit via requirement__ReqHygieneManagerAfterPermit
  goal__NoUnlicensedOperation --> requirement__ReqKeepLedgerCurrent
  %% derived goal__NoUnlicensedOperation -> requirement__ReqKeepLedgerCurrent via requirement__ReqKeepLedgerCurrent
  goal__NoUnlicensedOperation --> requirement__ReqPermitOnlyCompliantFacility
  %% derived goal__NoUnlicensedOperation -> requirement__ReqPermitOnlyCompliantFacility via requirement__ReqPermitOnlyCompliantFacility
  goal__NoUnlicensedOperation --> requirement__ReqPlannedMonitoring
  %% derived goal__NoUnlicensedOperation -> requirement__ReqPlannedMonitoring via requirement__ReqPlannedMonitoring
  goal__NoUnlicensedOperation --> requirement__ReqRenewBeforeExpiry
  %% derived goal__NoUnlicensedOperation -> requirement__ReqRenewBeforeExpiry via requirement__ReqRenewBeforeExpiry
  goal__NoUnlicensedOperation --> requirement__ReqStopOperationOnSanction
  %% derived goal__NoUnlicensedOperation -> requirement__ReqStopOperationOnSanction via requirement__ReqStopOperationOnSanction
  goal__TimelyDecision --> requirement__ReqClassifyBeforeConstruction
  %% derived goal__TimelyDecision -> requirement__ReqClassifyBeforeConstruction via requirement__ReqClassifyBeforeConstruction
  goal__TimelyDecision --> requirement__ReqFeeNotRefunded
  %% derived goal__TimelyDecision -> requirement__ReqFeeNotRefunded via requirement__ReqFeeNotRefunded
  goal__TimelyDecision --> requirement__ReqNotificationWithoutStandard
  %% derived goal__TimelyDecision -> requirement__ReqNotificationWithoutStandard via requirement__ReqNotificationWithoutStandard
  goal__TimelyDecision --> requirement__ReqPermitOnlyCompliantFacility
  %% derived goal__TimelyDecision -> requirement__ReqPermitOnlyCompliantFacility via requirement__ReqPermitOnlyCompliantFacility
  goal__TimelyDecision --> requirement__ReqReceiveBothChannels
  %% derived goal__TimelyDecision -> requirement__ReqReceiveBothChannels via requirement__ReqReceiveBothChannels
  requirement__ReqClassifyBeforeConstruction --> buc__BucConsultation
  requirement__ReqDueProcessDisposition --> buc__BucDisposition
  requirement__ReqFeeNotRefunded --> buc__BucPermitApplication
  requirement__ReqHygieneManagerAfterPermit --> buc__BucInspectionDecision
  requirement__ReqKeepLedgerCurrent --> buc__BucPermitChanges
  requirement__ReqNoAppealContents -.-> buc__BucDisposition
  requirement__ReqNoFoodLabeling -.-> buc__BucConsultation
  requirement__ReqNoFoodPoisoningInvestigation -.-> buc__BucDisposition
  requirement__ReqNoImportedFood -.-> buc__BucConsultation
  requirement__ReqNoOtherPermits -.-> buc__BucConsultation
  requirement__ReqNotificationWithoutStandard --> buc__BucBusinessNotification
  requirement__ReqPermitOnlyCompliantFacility --> buc__BucInspectionDecision
  requirement__ReqPlannedMonitoring --> buc__BucMonitoring
  requirement__ReqPublishViolations --> buc__BucDisposition
  requirement__ReqReceiveBothChannels --> buc__BucPermitApplication
  requirement__ReqRenewBeforeExpiry --> buc__BucRenewal
  requirement__ReqStopOperationOnSanction --> buc__BucDisposition
  classDef requirement fill:#fff3bf,stroke:#e67700,color:#000;
  classDef goal fill:#fff3bf,stroke:#e67700,color:#000;
  classDef buc fill:#d3f9d8,stroke:#2b8a3e,color:#000;
```

### 不利益処分を手続どおりに行う

| ID | 要求 | 優先度 | 関係者 | 目的 | 受け入れ条件 | 出典 | BUC |
|---|---|---|---|---|---|---|---|
| `ReqDueProcessDisposition` | 不利益処分は処分基準に沿い、意見陳述の手続と理由の提示を経て行う | must | 営業者、都道府県知事等 | 不利益処分を手続どおりに行う | 取消はすべて聴聞を経ており、意見陳述を省いた処分は整備改善命令だけで、決定した処分には理由が示されている。 | 処分基準 (egov-gyousei-tetsuzukihou-405AC0000000088.txt:197); 取消は聴聞、ほかは弁明の機会の付与 (egov-gyousei-tetsuzukihou-405AC0000000088.txt:207, :216); 技術的基準への適合を命じる処分の例外 (egov-gyousei-tetsuzukihou-405AC0000000088.txt:224); 理由の提示 (egov-gyousei-tetsuzukihou-405AC0000000088.txt:231) | 不利益処分と公表 |
| `ReqPublishViolations` | 処分を決めた違反を一定期間公表する | should | 保健所 許可担当職員 | 不利益処分を手続どおりに行う | 公表は処分の決定の後に始まり、期間が過ぎると終わる。 | 違反した者の名称等を公表 (egov-shokuhin-eiseihou-322AC0000000233.txt:473); 7日間をくだらない期間 (nerima-ku-ihan-kouhyou.md:42) | 不利益処分と公表 |

### 無許可営業をなくす

| ID | 要求 | 優先度 | 関係者 | 目的 | 受け入れ条件 | 出典 | BUC |
|---|---|---|---|---|---|---|---|
| `ReqClassifyBeforeConstruction` | 工事の前に営業の種類を判定し、許可業種なら施設基準を説明する | should | 営業者、食品衛生監視員 | 無許可営業をなくす、開店予定日までに許可する | 相談ごとに、許可・届出・不要のいずれかが営業の種類から一意に決まる。 | 工事をはじめる前に…相談 (osaka-city-eigyou-kyoka-tetsuzuki.md:88); 業種の選択に関すること (mhlw-shinsei-system-index.md:233); 一施設一許可 (mhlw-eigyou-kisei-qa.txt:131) | 事前相談と営業の種類の判定 |
| `ReqHygieneManagerAfterPermit` | 申請時に食品衛生責任者が未定なら、許可後速やかに定めさせる | should | 営業者 | 無許可営業をなくす | 有効な許可で食品衛生責任者のいない状態は、いずれ責任者の選定か許可の終了に至る。 | 食品衛生責任者を定めること (egov-shokuhin-eiseihou-shikoukisoku-323M40000100023.txt:3220); 営業の許可後速やかに (mhlw-2019-seishokuhatsu1227-2-sekou.txt:431, :435) | 施設検査と許可・不許可 |
| `ReqKeepLedgerCurrent` | 許可後の変更・承継・廃業を届出で台帳に反映する | must | 営業者、保健所 許可担当職員 | 無許可営業をなくす | 変更届と地位承継届は終わっていない許可にだけ出され、廃業した許可では営業していない。 | 変更の届出 (egov-shokuhin-eiseihou-shikoukisoku-323M40000100023.txt:950); 地位の承継 (egov-shokuhin-eiseihou-322AC0000000233.txt:412-413); 廃業の届出 (egov-shokuhin-eiseihou-shikoukisoku-323M40000100023.txt:952) | 変更届・地位の承継・廃業届 |
| `ReqPermitOnlyCompliantFacility` | 施設基準に適合した施設にだけ許可し、不許可には理由を示す | must | 営業者、食品衛生監視員、都道府県知事等 | 無許可営業をなくす、開店予定日までに許可する | 許可した申請の施設検査は適合であり、不許可の申請には理由が示されている。 | 基準に合うと認めるときは、許可をしなければならない (egov-shokuhin-eiseihou-322AC0000000233.txt:402); 理由の提示 (egov-gyousei-tetsuzukihou-405AC0000000088.txt:176); 改善後、再検査 (tokyo-minamitama-kyoka-kigen-manryou.md:226) | 施設検査と許可・不許可 |
| `ReqPlannedMonitoring` | 毎年度の計画を公表し、計画に沿って立入検査する | must | 食品衛生監視員、保健所 許可担当職員 | 無許可営業をなくす | 立入検査は公表済の計画の下でだけ予定される。 | 監視指導計画 (egov-shokuhin-eiseihou-322AC0000000233.txt:181, :192); 計画の定めるところにより監視指導 (egov-shokuhin-eiseihou-322AC0000000233.txt:236) | 監視指導計画と立入検査 |
| `ReqRenewBeforeExpiry` | 満了の前に通知し、満了日までに更新を終える。失効した許可は新規扱いとする | must | 営業者、保健所 許可担当職員 | 無許可営業をなくす | 有効期間を書き換えるのは有効な許可だけで、失効した許可では営業していない。 | 五年を下らない有効期間 (egov-shokuhin-eiseihou-322AC0000000233.txt:410); 満了通知書 (shizuoka-city-kyoka-kikan-manryou.md:37); 満了日までに手続きを完了 (osaka-city-eigyou-kyoka-tetsuzuki.md:63); 期限を過ぎて営業すると無許可営業 (shizuoka-city-kyoka-kikan-manryou.md:36) | 有効期間の満了と更新 |
| `ReqStopOperationOnSanction` | 停止中と、取消・失効・廃業の後は営業させない | must | 都道府県知事等、食品衛生監視員 | 無許可営業をなくす | 停止中・取消・失効・廃業の許可には営業中の記録がない。 | 営業の禁止・停止 (egov-shokuhin-eiseihou-322AC0000000233.txt:429, :432); 無許可営業 (shizuoka-city-kyoka-kikan-manryou.md:36; egov-shokuhin-eiseihou-322AC0000000233.txt:542) | 不利益処分と公表 |

### 開店予定日までに許可する

| ID | 要求 | 優先度 | 関係者 | 目的 | 受け入れ条件 | 出典 | BUC |
|---|---|---|---|---|---|---|---|
| `ReqClassifyBeforeConstruction` | 工事の前に営業の種類を判定し、許可業種なら施設基準を説明する | should | 営業者、食品衛生監視員 | 無許可営業をなくす、開店予定日までに許可する | 相談ごとに、許可・届出・不要のいずれかが営業の種類から一意に決まる。 | 工事をはじめる前に…相談 (osaka-city-eigyou-kyoka-tetsuzuki.md:88); 業種の選択に関すること (mhlw-shinsei-system-index.md:233); 一施設一許可 (mhlw-eigyou-kisei-qa.txt:131) | 事前相談と営業の種類の判定 |
| `ReqFeeNotRefunded` | 納めた手数料は、取下げ・不許可でも還付しない | must | 営業者 | 開店予定日までに許可する | 取下げ・不許可の申請にも手数料の納付の記録が残り、還付の手続はない。 | 既納の手数料は、還付しない (osaka-pref-shokuhin-eiseihou-sekou-jourei.md:40) | 営業許可の申請と受付 |
| `ReqNotificationWithoutStandard` | 届出は施設基準・施設検査・手数料・更新なしで受け付ける | must | 営業者 | 開店予定日までに許可する | 営業届は確認だけで受付済になり、有効期間による失効がない。 | 届出 (egov-shokuhin-eiseihou-322AC0000000233.txt:415); 要件（施設基準）はありません (mhlw-eigyou-todoke-sousetsu-shousai.txt:10); 更新の必要はありません (mhlw-eigyou-todoke-sousetsu-shousai.txt:11) | 営業届出 |
| `ReqPermitOnlyCompliantFacility` | 施設基準に適合した施設にだけ許可し、不許可には理由を示す | must | 営業者、食品衛生監視員、都道府県知事等 | 無許可営業をなくす、開店予定日までに許可する | 許可した申請の施設検査は適合であり、不許可の申請には理由が示されている。 | 基準に合うと認めるときは、許可をしなければならない (egov-shokuhin-eiseihou-322AC0000000233.txt:402); 理由の提示 (egov-gyousei-tetsuzukihou-405AC0000000088.txt:176); 改善後、再検査 (tokyo-minamitama-kyoka-kigen-manryou.md:226) | 施設検査と許可・不許可 |
| `ReqReceiveBothChannels` | 窓口と電子申請を同じ手順で受け付け、不備は補正を求める | must | 営業者、保健所 許可担当職員 | 開店予定日までに許可する | 窓口と電子のどちらの申請も、手数料の納付を確かめてから検査待になり、差戻しの間に取り下げられる。 | 申請書の記載事項 (egov-shokuhin-eiseihou-shikoukisoku-323M40000100023.txt:874-888); 補正を求め、又は…拒否 (egov-gyousei-tetsuzukihou-405AC0000000088.txt:173); 電子申請の状態と差戻し・取下げ (i2fas-faq.md:95, :106, :108) | 営業許可の申請と受付 |

## 4. やらないこと

| ID | 内容 | 理由 |
|---|---|---|
| `ReqNoAppealContents` | 審査請求と訴訟の中身は対象外とする | 審査請求ができる期間（処分を知った日の翌日から3月）とその教示だけを扱い、期間の経過で処分を確定として記録する。審査請求の審理、裁決、取消訴訟は審査庁・裁判所の手続で、調査もしていない。 |
| `ReqNoFoodLabeling` | 食品表示は対象外とする | 食品表示は食品表示法の手続で、営業の許可とは別の監視の対象になる。調査もしていない。 |
| `ReqNoFoodPoisoningInvestigation` | 食中毒の調査と、59条の廃棄命令は対象外とする | 食中毒の調査（法63条以降・施行令36〜37条）と、食品の廃棄・危害除去の命令（59条）は、許可の業務ではなく事件への対応で、調査もしていない。処分としては停止・取消と同じ手続を通る。 |
| `ReqNoImportedFood` | 輸入食品の届出は対象外とする | 輸入の営業は営業届出も要らず（施行令35条の2）、輸入の届出は検疫所が受ける国の手続で、保健所の許可業務ではない。判定では届出不要の一つとして扱うだけにする。 |
| `ReqNoOtherPermits` | 他法令の許可・届出は対象外とする | 深夜酒類提供の届出、風俗営業の許可、酒類の販売免許、防火の届出は、警察・税務署・消防の手続で、食品衛生法の許可と同時に進んでも互いの要件にはならない。調査もしていない。 |

## 5. ユーザージャーニー

次の図は業務の流れである。業務ごとに BUC をまとめ、ある BUC から次の BUC への引き渡しを、引き渡すイベントのラベルで示す。

```mermaid
%% rdra-ish diagram=business scope=whole model=samples/food-permit-ops
%% kinds: business buc
%% omitted: actor requirement nfr flow step timer medium permission usecase screen event api entity condition
flowchart LR
  subgraph business__AfterPermit["許可後・届出の手続"]
    %% desc business__AfterPermit: 変更届、地位の承継、廃業届と、許可の要らない営業の営業届出。
    buc__BucBusinessNotification["営業届出"]:::buc
    %% desc buc__BucBusinessNotification: 許可業種でも届出不要の営業でもない営業は、あらかじめ営業所の名称・所在地などを届け出る。許可と違って施設基準はなく、施設検査も手数料も更新もない。届出営業者も食品衛生責任者を定め、廃業したら届け出る。
    buc__BucPermitChanges["変更届・地位の承継・廃業届"]:::buc
    %% desc buc__BucPermitChanges: 許可営業者は申請書の記載事項が変わったら速やかに（東京都は10日以内）変更届を出す。移転や営業者の交代は変更ではなく新規の申請になる。営業の譲渡・相続・合併・分割では許可営業者の地位が承継され、承継した者が遅滞なく届け出る。廃業したら許可証を添えて廃業届を出す。
  end
  subgraph business__Licensing["営業許可"]
    %% desc business__Licensing: 事前相談と営業の種類の判定、許可申請の受付と審査、施設検査、許可・不許可、有効期間の満了と更新。
    buc__BucConsultation["事前相談と営業の種類の判定"]:::buc
    %% desc buc__BucConsultation: 営業者は施設の工事の前に図面を持って保健所に相談する。食品衛生監視員は営業の内容から、許可が要るか（施行令35条の32業種）、届出でよいか、どちらも要らないか（施行令35条の2）を判定し、許可業種なら施設基準と食品衛生責任者を説明する。
    buc__BucInspectionDecision["施設検査と許可・不許可"]:::buc
    %% desc buc__BucInspectionDecision: 食品衛生監視員が施設を検査し、施設基準に合わなければ改善を指示して再検査する。行政庁は基準に合うと認めれば許可しなければならず、合わなければ理由を示して不許可にする。許可すると台帳に登録して許可証を交付し、営業者は許可証を掲示して営業を始める。食品衛生責任者が申請時に未定なら、許可後速やかに定める。
    buc__BucPermitApplication["営業許可の申請と受付"]:::buc
    %% desc buc__BucPermitApplication: 営業者は開店の2〜3週間前（東京都は工事完成の10日前ころ）に、申請書と図面を保健所の窓口に出すか、食品衛生申請等システムで電子申請し、条例の手数料を納める。保健所は記載事項と添付書類を確かめ、足りなければ補正を求め（差戻し）、整えば施設検査に進める（検査待）。申請者は差戻しの間に取り下げられる。
    buc__BucRenewal["有効期間の満了と更新"]:::buc
    %% desc buc__BucRenewal: 許可には5年以上の有効期間が付く。満了の前に保健所が通知し（静岡市は2か月前）、営業者は満了前に継続（更新）の申請をする。更新でも施設検査があり、満了日までに許可を終える。更新しないまま満了すると許可は失効し、営業を続けると無許可営業になる。失効した許可は更新できず、新規の申請からやり直す。
  end
  subgraph business__Supervision["監視指導と行政処分"]
    %% desc business__Supervision: 監視指導計画と立入検査、整備改善命令・営業停止・許可取消と、その前の聴聞・弁明、違反の公表。
    buc__BucDisposition["不利益処分と公表"]:::buc
    %% desc buc__BucDisposition: 施設基準違反などで、行政庁は施設の整備改善を命じ、営業を停止し、許可を取り消す。取消の前には聴聞、ほかは弁明の機会を与える。ただし計測などで確かめた技術的基準の不充足に基準どおりにせよと命じる処分は、意見陳述を省ける。処分は理由を示して書面で行い、審査請求ができることを教える。違反者は公表する（練馬区は7日以上）。停止は定めた期間が過ぎると終わる。
    buc__BucMonitoring["監視指導計画と立入検査"]:::buc
    %% desc buc__BucMonitoring: 都道府県知事等は毎年度、翌年度の監視指導計画を定めて公表し、計画に沿って食品衛生監視員に営業の場所を立入検査させる。違反を見つけたら、処分の起案に回す。
  end
  buc__BucMonitoring -->|立入検査での違反の発見| buc__BucDisposition
  %% derived buc__BucMonitoring -> buc__BucDisposition via event__EvViolationFound
  classDef business fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef buc fill:#d3f9d8,stroke:#2b8a3e,color:#000;
```

この後の節は BUC ごとの業務フローである。手順、分岐、例外と、各手順を担う UC を示す。

### 事前相談と営業の種類の判定

```mermaid
%% rdra-ish diagram=flow scope=buc:BucConsultation model=samples/food-permit-ops
%% kinds: flow step lane usecase
%% omitted: buc
flowchart TD
  subgraph flow__ConsultationFlow["事前相談の流れ"]
    subgraph lane__ConsultationFlow__BusinessOperator["営業者"]
      %% lane lane__ConsultationFlow__BusinessOperator: actor__BusinessOperator
      step__ConsultStep["図面を持って相談（工事の前）"]:::step
    end
    subgraph lane__ConsultationFlow__FoodSanitationInspector["食品衛生監視員"]
      %% lane lane__ConsultationFlow__FoodSanitationInspector: actor__FoodSanitationInspector
      step__ClassifyStep["許可・届出・不要の判定"]:::step
      step__PlanReviewStep["図面の確認と施設基準の説明"]:::step
    end
  end
  usecase__ClassifyBusiness("営業の種類の判定"):::usecase
  %% desc usecase__ClassifyBusiness: 営業の種類から、許可が必要か、届出が必要か、どちらも要らないかを判定する。結果は営業の種類で決まり、監視員が選ぶものではない（各結果に条件）。
  usecase__RequestConsultation("事前相談の申込み"):::usecase
  %% desc usecase__RequestConsultation: 営業者が施設の図面と営業の内容を持って保健所に相談する。相談で営業の種類を伝える。
  usecase__ReviewFloorPlan("図面の確認と施設基準の説明"):::usecase
  %% desc usecase__ReviewFloorPlan: 食品衛生監視員が図面を施設基準（共通基準・業種別基準）と照らし、足りない設備と、食品衛生責任者の資格（調理師・栄養士等か講習会の受講）を説明する（参照のみ）。
  step__ClassifyStep -.-> usecase__ClassifyBusiness
  step__ClassifyStep -->|repeats| step__PlanReviewStep
  step__ConsultStep -.-> usecase__RequestConsultation
  step__ConsultStep -->|precedes| step__PlanReviewStep
  step__PlanReviewStep -.-> usecase__ReviewFloorPlan
  step__PlanReviewStep -->|precedes| step__ClassifyStep
  classDef flow fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef step fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef lane fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef usecase fill:#d0ebff,stroke:#1864ab,color:#000;
```

### 営業許可の申請と受付

```mermaid
%% rdra-ish diagram=flow scope=buc:BucPermitApplication model=samples/food-permit-ops
%% kinds: flow step lane usecase event
%% omitted: buc
flowchart TD
  subgraph flow__PermitApplicationFlow["許可申請の流れ"]
    subgraph lane__PermitApplicationFlow__BusinessOperator["営業者"]
      %% lane lane__PermitApplicationFlow__BusinessOperator: actor__BusinessOperator
      step__FeeStep["手数料の納付"]:::step
      step__SubmitStep["申請書の提出（窓口・電子）"]:::step
      step__WithdrawStep["取下げ"]:::step
    end
    subgraph lane__PermitApplicationFlow__PermitClerk["保健所 許可担当職員"]
      %% lane lane__PermitApplicationFlow__PermitClerk: actor__PermitClerk
      step__ReturnStep["差戻し・補正"]:::step
      step__ReviewStep["申請内容の確認（申請内容確認待）"]:::step
    end
  end
  usecase__PayPermitFee("許可申請手数料の納付"):::usecase
  %% desc usecase__PayPermitFee: 営業者が条例の手数料を保健所に納める（電子申請でも別途納付）。納めた手数料は還付されない。
  usecase__ReceiveOnlineApplication("電子申請の受付"):::usecase
  %% desc usecase__ReceiveOnlineApplication: 食品衛生申請等システムで提出された新規の許可申請を、管轄の保健所の確認待ち（申請内容確認待）として受け付ける。
  usecase__ResubmitApplication("補正した申請の再提出"):::usecase
  %% desc usecase__ResubmitApplication: 営業者が差戻しを受けて申請を直し、再び確認待にする。
  usecase__ReturnApplication("差戻し（補正の求め）"):::usecase
  %% desc usecase__ReturnApplication: 記載の誤りや添付書類の不足があれば、期間を定めて補正を求め、申請を差し戻す。電子申請では申請者の依頼による差戻しもある。
  usecase__ReviewApplication("申請内容の確認と受理"):::usecase
  %% desc usecase__ReviewApplication: 許可担当が記載事項（営業者・施設・営業の種類・食品衛生責任者・図面・欠格事由の有無）と手数料の納付を確かめ、施設検査に進める（検査待）。
  usecase__SubmitCounterApplication("許可申請書の提出（窓口）"):::usecase
  %% desc usecase__SubmitCounterApplication: 営業者が許可申請書（新規）に図面と食品衛生責任者の資格を証する書類を添えて、保健所の窓口に出す。
  usecase__WithdrawApplication("申請の取下げ"):::usecase
  %% desc usecase__WithdrawApplication: 営業者が許可・不許可の前に申請を取り下げる。電子申請は保健所の差戻しを受けてから取り下げる。手数料は戻らない。
  event__EvOnlineApplicationSubmitted>"電子申請の提出（整理番号 LIC）"]:::event
  step__FeeStep -.-> usecase__PayPermitFee
  step__FeeStep -->|precedes| step__ReviewStep
  step__ReturnStep -->|branches| step__WithdrawStep
  step__ReturnStep -.-> usecase__ResubmitApplication
  step__ReturnStep -.-> usecase__ReturnApplication
  step__ReturnStep -->|repeats| step__ReviewStep
  step__ReviewStep -.-> usecase__ReviewApplication
  step__ReviewStep -.->|excepts| step__ReturnStep
  step__SubmitStep -.-> event__EvOnlineApplicationSubmitted
  step__SubmitStep -.-> usecase__ReceiveOnlineApplication
  step__SubmitStep -.-> usecase__SubmitCounterApplication
  step__SubmitStep -->|precedes| step__FeeStep
  step__WithdrawStep -.-> usecase__WithdrawApplication
  classDef flow fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef step fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef lane fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef usecase fill:#d0ebff,stroke:#1864ab,color:#000;
  classDef event fill:#ffdeeb,stroke:#a61e4d,color:#000;
```

### 施設検査と許可・不許可

```mermaid
%% rdra-ish diagram=flow scope=buc:BucInspectionDecision model=samples/food-permit-ops
%% kinds: flow step lane usecase
%% omitted: buc
flowchart TD
  subgraph flow__InspectionDecisionFlow["施設検査と許可の流れ"]
    subgraph lane__InspectionDecisionFlow__BusinessOperator["営業者"]
      %% lane lane__InspectionDecisionFlow__BusinessOperator: actor__BusinessOperator
      step__HygieneManagerStep["食品衛生責任者の選定"]:::step
      step__ImproveStep["改善と再検査"]:::step
      step__OpenStep["許可証の掲示と営業の開始"]:::step
    end
    subgraph lane__InspectionDecisionFlow__FoodSanitationInspector["食品衛生監視員"]
      %% lane lane__InspectionDecisionFlow__FoodSanitationInspector: actor__FoodSanitationInspector
      step__InspectStep["施設検査"]:::step
    end
    subgraph lane__InspectionDecisionFlow__PermitAuthority["都道府県知事等（保健所設置市長・特別区長）"]
      %% lane lane__InspectionDecisionFlow__PermitAuthority: actor__PermitAuthority
      step__DenyStep["不許可（理由の提示）"]:::step
      step__GrantStep["許可の決定"]:::step
    end
    subgraph lane__InspectionDecisionFlow__PermitClerk["保健所 許可担当職員"]
      %% lane lane__InspectionDecisionFlow__PermitClerk: actor__PermitClerk
      step__RegisterStep["台帳登録と許可証の交付"]:::step
      step__ScheduleStep["検査日の調整"]:::step
    end
  end
  usecase__AppointHygieneManager("食品衛生責任者の選定の届出"):::usecase
  %% desc usecase__AppointHygieneManager: 申請時に食品衛生責任者を定めていなかった営業者が、許可後に定めて（講習会を受けて）届け出る。
  usecase__ConductInspection("施設検査の実施"):::usecase
  %% desc usecase__ConductInspection: 食品衛生監視員が施設を施設基準（共通基準・業種別基準）と照らし、衛生管理計画を確かめる。合わなければ改善を指示する。
  usecase__DenyPermit("不許可の決定"):::usecase
  %% desc usecase__DenyPermit: 施設基準に合わないか、補正に応じないか、欠格事由に当たる申請を、理由を示し、審査請求ができることを書面で教えて拒否する。
  usecase__GrantPermit("許可の決定"):::usecase
  %% desc usecase__GrantPermit: 施設検査で施設基準に合うと認めた新規の申請に許可を与え、有効期間（5年以上）を付ける。
  usecase__IssueCertificate("許可証の交付"):::usecase
  %% desc usecase__IssueCertificate: 許可証を営業者に交付する。
  usecase__RegisterPermit("許可台帳への登録"):::usecase
  %% desc usecase__RegisterPermit: 許可の決定を受けて許可番号を付け、許可台帳に登録する。電子申請ではシステムの表示を発行済にする。
  usecase__RequestReinspection("改善の報告と再検査の申込み"):::usecase
  %% desc usecase__RequestReinspection: 営業者が指示された箇所を直し、再検査を申し込む。
  usecase__ScheduleInspection("施設検査の日程調整"):::usecase
  %% desc usecase__ScheduleInspection: 手数料の納付を確かめた検査待の申請について、施設検査の日を決める。
  usecase__StartBusiness("営業の開始・再開"):::usecase
  %% desc usecase__StartBusiness: 営業者が許可証を掲示して営業を始める。営業停止の期間が終わった後の再開も同じ。
  step__DenyStep -.-> usecase__DenyPermit
  step__GrantStep -.-> usecase__GrantPermit
  step__GrantStep -->|precedes| step__RegisterStep
  step__HygieneManagerStep -.-> usecase__AppointHygieneManager
  step__ImproveStep -.-> usecase__RequestReinspection
  step__ImproveStep -->|repeats| step__InspectStep
  step__InspectStep -->|branches| step__DenyStep
  step__InspectStep -.-> usecase__ConductInspection
  step__InspectStep -.->|excepts| step__ImproveStep
  step__InspectStep -->|precedes| step__GrantStep
  step__OpenStep -.-> usecase__StartBusiness
  step__RegisterStep -->|branches| step__HygieneManagerStep
  step__RegisterStep -.-> usecase__IssueCertificate
  step__RegisterStep -.-> usecase__RegisterPermit
  step__RegisterStep -->|precedes| step__OpenStep
  step__ScheduleStep -.-> usecase__ScheduleInspection
  step__ScheduleStep -->|precedes| step__InspectStep
  classDef flow fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef step fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef lane fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef usecase fill:#d0ebff,stroke:#1864ab,color:#000;
```

### 有効期間の満了と更新

```mermaid
%% rdra-ish diagram=flow scope=buc:BucRenewal model=samples/food-permit-ops
%% kinds: flow step lane timer usecase
%% omitted: buc
flowchart TD
  subgraph flow__RenewalFlow["更新の流れ"]
    step__ExpiryStep["満了による失効"]:::step
    subgraph lane__RenewalFlow__BusinessOperator["営業者"]
      %% lane lane__RenewalFlow__BusinessOperator: actor__BusinessOperator
      step__RenewalApplyStep["継続の申請（満了の1か月〜2週間前）"]:::step
    end
    subgraph lane__RenewalFlow__FoodSanitationInspector["食品衛生監視員"]
      %% lane lane__RenewalFlow__FoodSanitationInspector: actor__FoodSanitationInspector
      step__RenewalInspectStep["更新前の施設検査"]:::step
    end
    subgraph lane__RenewalFlow__PermitAuthority["都道府県知事等（保健所設置市長・特別区長）"]
      %% lane lane__RenewalFlow__PermitAuthority: actor__PermitAuthority
      step__RenewalDecisionStep["更新の許可と有効期間の書換え"]:::step
    end
    subgraph lane__RenewalFlow__PermitClerk["保健所 許可担当職員"]
      %% lane lane__RenewalFlow__PermitClerk: actor__PermitClerk
      step__NoticeStep["満了通知（2か月前）"]:::step
    end
  end
  timer__ExpiryNoticeDue(("満了通知の発送（有効期間満了の2か月前）")):::timer
  timer__PermitTermEnd(("有効期間の満了（5年以上、自治体が定める）")):::timer
  usecase__ApplyRenewal("継続（更新）の申請"):::usecase
  %% desc usecase__ApplyRenewal: 有効な許可の営業者が、満了の前に継続の許可を申請する（窓口・電子）。手数料と施設検査は新規と同じ手順で進む。
  usecase__ConductInspection("施設検査の実施"):::usecase
  %% desc usecase__ConductInspection: 食品衛生監視員が施設を施設基準（共通基準・業種別基準）と照らし、衛生管理計画を確かめる。合わなければ改善を指示する。
  usecase__ExpirePermit("有効期間の満了による失効"):::usecase
  %% desc usecase__ExpirePermit: 更新されないまま有効期間が満了した許可を失効させる。営業者は営業をやめ、続けるなら新規の許可を受ける。
  usecase__ExtendPermitTerm("有効期間の更新"):::usecase
  %% desc usecase__ExtendPermitTerm: 更新の許可を受けて、台帳の有効期間を新しい期間に書き換え、満了通知の記録を消す。
  usecase__RenewPermit("更新の許可の決定"):::usecase
  %% desc usecase__RenewPermit: 施設検査で施設基準に合うと認めた継続の申請に、許可がまだ有効なうちに許可を与える。
  usecase__ScheduleInspection("施設検査の日程調整"):::usecase
  %% desc usecase__ScheduleInspection: 手数料の納付を確かめた検査待の申請について、施設検査の日を決める。
  usecase__SendExpiryNotice("満了通知の発送"):::usecase
  %% desc usecase__SendExpiryNotice: 有効期間が満了する前に、許可営業者に満了の時期と更新の手続を知らせる。
  step__ExpiryStep -.-> timer__PermitTermEnd
  step__ExpiryStep -.-> usecase__ExpirePermit
  step__NoticeStep -.-> timer__ExpiryNoticeDue
  step__NoticeStep -.-> usecase__SendExpiryNotice
  step__NoticeStep -.->|excepts| step__ExpiryStep
  step__NoticeStep -->|precedes| step__RenewalApplyStep
  step__RenewalApplyStep -.-> usecase__ApplyRenewal
  step__RenewalApplyStep -->|precedes| step__RenewalInspectStep
  step__RenewalDecisionStep -.-> usecase__ExtendPermitTerm
  step__RenewalDecisionStep -.-> usecase__RenewPermit
  step__RenewalInspectStep -.-> usecase__ConductInspection
  step__RenewalInspectStep -.-> usecase__ScheduleInspection
  step__RenewalInspectStep -->|precedes| step__RenewalDecisionStep
  classDef flow fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef step fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef lane fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef timer fill:#ffe8cc,stroke:#d9480f,color:#000;
  classDef usecase fill:#d0ebff,stroke:#1864ab,color:#000;
```

### 変更届・地位の承継・廃業届

```mermaid
%% rdra-ish diagram=flow scope=buc:BucPermitChanges model=samples/food-permit-ops
%% kinds: flow step lane usecase
%% omitted: buc
flowchart TD
  subgraph flow__PermitChangesFlow["許可後の届出の流れ"]
    subgraph lane__PermitChangesFlow__BusinessOperator["営業者"]
      %% lane lane__PermitChangesFlow__BusinessOperator: actor__BusinessOperator
      step__ChangeStep["変更届（10日以内）"]:::step
      step__ClosureStep["廃業届（10日以内、許可証を添える）"]:::step
      step__SuccessionStep["地位承継届（遅滞なく）"]:::step
    end
    subgraph lane__PermitChangesFlow__PermitClerk["保健所 許可担当職員"]
      %% lane lane__PermitChangesFlow__PermitClerk: actor__PermitClerk
      step__ChangeReviewStep["変更の確認（新規申請の要否）"]:::step
    end
  end
  usecase__AcceptSuccessionNotice("地位承継届の受理"):::usecase
  %% desc usecase__AcceptSuccessionNotice: 許可担当が承継を証する書面を確かめ、台帳の営業者を書き換える。
  usecase__FileChangeNotice("変更届の提出"):::usecase
  %% desc usecase__FileChangeNotice: 許可営業者が、申請書の記載事項（営業者の氏名・住所、施設の名称、食品衛生責任者など）や設備の変更を届け出る。
  usecase__FileClosureNotice("廃業届の提出"):::usecase
  %% desc usecase__FileClosureNotice: 許可営業者が営業をやめた日から10日以内に、許可証を添えて廃業を届け出る。許可はここで終わる。
  usecase__FileSuccessionNotice("地位承継届の提出"):::usecase
  %% desc usecase__FileSuccessionNotice: 営業を譲り受けた者、相続人、合併・分割で営業を承継した法人が、承継を証する書面を添えて届け出る。
  usecase__ReviewChangeNotice("変更届の確認"):::usecase
  %% desc usecase__ReviewChangeNotice: 許可担当が変更の内容を確かめ、届出で足りるか、新規の許可申請が必要かを判断する。
  step__ChangeReviewStep -.-> usecase__ReviewChangeNotice
  step__ChangeStep -->|branches| step__ClosureStep
  step__ChangeStep -->|branches| step__SuccessionStep
  step__ChangeStep -.-> usecase__FileChangeNotice
  step__ChangeStep -->|precedes| step__ChangeReviewStep
  step__ClosureStep -.-> usecase__FileClosureNotice
  step__SuccessionStep -.-> usecase__AcceptSuccessionNotice
  step__SuccessionStep -.-> usecase__FileSuccessionNotice
  classDef flow fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef step fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef lane fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef usecase fill:#d0ebff,stroke:#1864ab,color:#000;
```

### 営業届出

```mermaid
%% rdra-ish diagram=flow scope=buc:BucBusinessNotification model=samples/food-permit-ops
%% kinds: flow step lane usecase event
%% omitted: buc
flowchart TD
  subgraph flow__NotificationFlow["営業届出の流れ"]
    subgraph lane__NotificationFlow__BusinessOperator["営業者"]
      %% lane lane__NotificationFlow__BusinessOperator: actor__BusinessOperator
      step__NotifiedClosureStep["廃業届"]:::step
      step__NotifyStep["営業届の提出（営業の前）"]:::step
    end
    subgraph lane__NotificationFlow__PermitClerk["保健所 許可担当職員"]
      %% lane lane__NotificationFlow__PermitClerk: actor__PermitClerk
      step__AcceptStep["受付（受付済）"]:::step
    end
  end
  usecase__AcceptNotification("営業届の受付"):::usecase
  %% desc usecase__AcceptNotification: 許可担当が記載を確かめて受け付ける。施設基準による審査と施設検査はない。
  usecase__FileNotificationClosure("届出営業の廃業届"):::usecase
  %% desc usecase__FileNotificationClosure: 届出営業者が営業をやめたことを届け出る。
  usecase__ReceiveOnlineNotification("電子届出の受付"):::usecase
  %% desc usecase__ReceiveOnlineNotification: 食品衛生申請等システムで出された営業届を、届出内容確認待として受け付ける。
  usecase__SubmitCounterNotification("営業届の提出（窓口）"):::usecase
  %% desc usecase__SubmitCounterNotification: 営業者が営業を始める前に、保健所の窓口に営業届を出す。
  event__EvOnlineNotificationSubmitted>"電子届出の提出（整理番号 NIF）"]:::event
  step__AcceptStep -.-> usecase__AcceptNotification
  step__AcceptStep -->|precedes| step__NotifiedClosureStep
  step__NotifiedClosureStep -.-> usecase__FileNotificationClosure
  step__NotifyStep -.-> event__EvOnlineNotificationSubmitted
  step__NotifyStep -.-> usecase__ReceiveOnlineNotification
  step__NotifyStep -.-> usecase__SubmitCounterNotification
  step__NotifyStep -->|precedes| step__AcceptStep
  classDef flow fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef step fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef lane fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef usecase fill:#d0ebff,stroke:#1864ab,color:#000;
  classDef event fill:#ffdeeb,stroke:#a61e4d,color:#000;
```

### 監視指導計画と立入検査

```mermaid
%% rdra-ish diagram=flow scope=buc:BucMonitoring model=samples/food-permit-ops
%% kinds: flow step lane timer usecase
%% omitted: buc
flowchart TD
  subgraph flow__MonitoringFlow["監視指導の流れ"]
    subgraph lane__MonitoringFlow__FoodSanitationInspector["食品衛生監視員"]
      %% lane lane__MonitoringFlow__FoodSanitationInspector: actor__FoodSanitationInspector
      step__ViolationStep["違反の記録と処分の起案へ"]:::step
      step__VisitStep["計画に沿った立入検査"]:::step
    end
    subgraph lane__MonitoringFlow__PermitClerk["保健所 許可担当職員"]
      %% lane lane__MonitoringFlow__PermitClerk: actor__PermitClerk
      step__PlanStep["翌年度の計画の策定"]:::step
      step__PublishPlanStep["計画の公表"]:::step
    end
  end
  timer__AnnualPlanning(("翌年度の監視指導計画の策定（毎年度）")):::timer
  usecase__DraftMonitoringPlan("監視指導計画の策定"):::usecase
  %% desc usecase__DraftMonitoringPlan: 重点的に監視指導する項目、自主的な衛生管理の指導、関係機関との連携を定めた翌年度の計画を作る。
  usecase__PublishMonitoringPlan("監視指導計画の公表"):::usecase
  %% desc usecase__PublishMonitoringPlan: 定めた計画を公表し、厚生労働大臣と内閣総理大臣に報告する。
  usecase__RecordCompliance("違反なしの記録"):::usecase
  %% desc usecase__RecordCompliance: 立入検査で違反がなかったことを記録する。
  usecase__RecordViolation("違反の記録"):::usecase
  %% desc usecase__RecordViolation: 立入検査で施設基準違反などを見つけたことを記録し、不利益処分の起案に回す。
  usecase__ScheduleSiteVisit("立入検査の予定"):::usecase
  %% desc usecase__ScheduleSiteVisit: 公表した計画の重点項目と件数に沿って、立入検査する施設を決める。
  step__PlanStep -.-> timer__AnnualPlanning
  step__PlanStep -.-> usecase__DraftMonitoringPlan
  step__PlanStep -->|precedes| step__PublishPlanStep
  step__PublishPlanStep -.-> usecase__PublishMonitoringPlan
  step__PublishPlanStep -->|precedes| step__VisitStep
  step__ViolationStep -.-> usecase__RecordViolation
  step__VisitStep -.-> usecase__RecordCompliance
  step__VisitStep -.-> usecase__ScheduleSiteVisit
  step__VisitStep -.->|excepts| step__ViolationStep
  classDef flow fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef step fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef lane fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef timer fill:#ffe8cc,stroke:#d9480f,color:#000;
  classDef usecase fill:#d0ebff,stroke:#1864ab,color:#000;
```

### 不利益処分と公表

```mermaid
%% rdra-ish diagram=flow scope=buc:BucDisposition model=samples/food-permit-ops
%% kinds: flow step lane timer usecase event
%% omitted: buc
flowchart TD
  subgraph flow__DispositionFlow["不利益処分の流れ"]
    step__AppealPeriodStep["審査請求期間（3月）の経過"]:::step
    step__SuspensionEndStep["停止期間の満了"]:::step
    subgraph lane__DispositionFlow__FoodSanitationInspector["食品衛生監視員"]
      %% lane lane__DispositionFlow__FoodSanitationInspector: actor__FoodSanitationInspector
      step__ProposeStep["処分の起案（処分基準）"]:::step
    end
    subgraph lane__DispositionFlow__PermitAuthority["都道府県知事等（保健所設置市長・特別区長）"]
      %% lane lane__DispositionFlow__PermitAuthority: actor__PermitAuthority
      step__DecideStep["処分の決定（理由の提示・教示）"]:::step
      step__DropStep["処分の見送り"]:::step
      step__OmittedStep["意見陳述を省いた整備改善命令"]:::step
    end
    subgraph lane__DispositionFlow__PermitClerk["保健所 許可担当職員"]
      %% lane lane__DispositionFlow__PermitClerk: actor__PermitClerk
      step__OpinionNoticeStep["聴聞・弁明の通知"]:::step
      step__OpinionStep["聴聞の実施・弁明書の受理"]:::step
      step__PublishStep["違反の公表（7日以上）"]:::step
    end
  end
  timer__AppealPeriodEnd(("審査請求期間の経過（処分を知った日の翌日から3月）")):::timer
  timer__PublicationPeriodEnd(("公表期間の経過（7日間以上）")):::timer
  timer__SuspensionTermEnd(("営業停止期間の満了（処分で定めた期間）")):::timer
  usecase__DecideRevocation("許可の取消の決定"):::usecase
  %% desc usecase__DecideRevocation: 聴聞を経て許可を取り消すと決める。理由を示し、審査請求ができることを書面で教える。営業者は取消から2年は新たな許可を受けられないことがある。
  usecase__DecideSuspension("営業停止の決定"):::usecase
  %% desc usecase__DecideSuspension: 弁明の機会を経て、期間を定めて営業を停止すると決める。理由を示し、審査請求ができることを書面で教える。
  usecase__DropDisposition("処分を見送る"):::usecase
  %% desc usecase__DropDisposition: 意見陳述の結果、処分しないと決める。
  usecase__EndPublication("公表の終了"):::usecase
  %% desc usecase__EndPublication: 公表期間（処分の翌日から7日以上）が過ぎたら公表をやめる。
  usecase__FinalizeDisposition("審査請求期間の経過"):::usecase
  %% desc usecase__FinalizeDisposition: 審査請求のないまま3月が過ぎたことを記録する。審査請求と訴訟の中身は対象外。
  usecase__HoldHearing("聴聞の実施"):::usecase
  %% desc usecase__HoldHearing: 聴聞の期日に名あて人の意見を聴き、記録する。取消の前には必ず行う。
  usecase__LiftSuspension("営業停止期間の満了"):::usecase
  %% desc usecase__LiftSuspension: 停止の期間が過ぎ、営業停止を終える。営業者は許可証を掲示して営業を再開できる。
  usecase__NotifyOpinionProcedure("聴聞・弁明の通知"):::usecase
  %% desc usecase__NotifyOpinionProcedure: 予定する処分の内容と根拠条項、原因となる事実、聴聞の期日または弁明書の提出期限を、名あて人に書面で通知する。
  usecase__OrderImprovement("施設の整備改善命令"):::usecase
  %% desc usecase__OrderImprovement: 施設基準に合わない設備の改善を命じる。計測などで確かめた不充足なら意見陳述を省いて命じられる。
  usecase__ProposeDisposition("処分の起案"):::usecase
  %% desc usecase__ProposeDisposition: 食品衛生監視員が違反の事実と処分基準から、処分の種類（整備改善命令・停止・取消）と意見陳述の手続（聴聞・弁明・省略）を起案する。
  usecase__PublishViolation("違反の公表"):::usecase
  %% desc usecase__PublishViolation: 処分を受けた営業者の名称、施設、処分の理由と内容をホームページと掲示場で公表する。
  usecase__ReceiveExplanation("弁明書の受理"):::usecase
  %% desc usecase__ReceiveExplanation: 停止や整備改善命令の前に、名あて人の弁明書を受け取る。期限までに出なければ、弁明の機会を与えたものとして手続を終える。
  usecase__RevokePermit("許可台帳への取消の記録"):::usecase
  %% desc usecase__RevokePermit: 取消の決定を台帳に記録し、許可を終わらせる。営業者は営業をやめる。
  usecase__SuspendBusiness("許可台帳への営業停止の記録"):::usecase
  %% desc usecase__SuspendBusiness: 停止の決定を台帳に記録する。営業者は停止の期間、営業をやめる。
  event__EvViolationFound>"立入検査での違反の発見"]:::event
  step__AppealPeriodStep -.-> timer__AppealPeriodEnd
  step__AppealPeriodStep -.-> usecase__FinalizeDisposition
  step__DecideStep -.-> usecase__DecideRevocation
  step__DecideStep -.-> usecase__DecideSuspension
  step__DecideStep -.-> usecase__OrderImprovement
  step__DecideStep -.-> usecase__RevokePermit
  step__DecideStep -.-> usecase__SuspendBusiness
  step__DecideStep -->|precedes| step__AppealPeriodStep
  step__DecideStep -->|precedes| step__PublishStep
  step__DecideStep -->|precedes| step__SuspensionEndStep
  step__DropStep -.-> usecase__DropDisposition
  step__OmittedStep -.-> usecase__OrderImprovement
  step__OpinionNoticeStep -.-> usecase__NotifyOpinionProcedure
  step__OpinionNoticeStep -->|precedes| step__OpinionStep
  step__OpinionStep -->|branches| step__DropStep
  step__OpinionStep -.-> usecase__HoldHearing
  step__OpinionStep -.-> usecase__ReceiveExplanation
  step__OpinionStep -->|precedes| step__DecideStep
  step__ProposeStep -->|branches| step__OmittedStep
  step__ProposeStep -.-> event__EvViolationFound
  step__ProposeStep -.-> usecase__ProposeDisposition
  step__ProposeStep -->|precedes| step__OpinionNoticeStep
  step__PublishStep -.-> timer__PublicationPeriodEnd
  step__PublishStep -.-> usecase__EndPublication
  step__PublishStep -.-> usecase__PublishViolation
  step__SuspensionEndStep -.-> timer__SuspensionTermEnd
  step__SuspensionEndStep -.-> usecase__LiftSuspension
  classDef flow fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef step fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef lane fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef timer fill:#ffe8cc,stroke:#d9480f,color:#000;
  classDef usecase fill:#d0ebff,stroke:#1864ab,color:#000;
  classDef event fill:#ffdeeb,stroke:#a61e4d,color:#000;
```

## 6. ビジネスルール（受け入れ条件）

`list --kind rule --format markdown` の出力である。各行が、システムが守る業務規則で、受け入れ試験の条件になる。

| kind | label | entities | rule | applies_to | site |
| --- | --- | --- | --- | --- | --- |
| condition | 許可業種（施行令35条）である | Consultation | Consultation.business_type in (飲食店営業, 食肉販売業, 菓子製造業) | ClassifyBusiness | samples/food-permit-ops/buc/buc_consultation.rdra:69 |
| condition | 届出業種である | Consultation | Consultation.business_type in (弁当販売業（届出）, 野菜果物販売業（届出）) | ClassifyBusiness | samples/food-permit-ops/buc/buc_consultation.rdra:72 |
| condition | 届出も要らない営業（施行令35条の2）である | Consultation | Consultation.business_type == 常温で保存できる包装食品の販売（届出不要） | ClassifyBusiness | samples/food-permit-ops/buc/buc_consultation.rdra:75 |
| condition | 取消ではない処分である | Disposition | Disposition.kind in (施設の整備改善命令, 営業の停止) | ProposeDisposition | samples/food-permit-ops/buc/buc_disposition.rdra:130 |
| condition | 施設の整備改善命令である | Disposition | Disposition.kind == 施設の整備改善命令 | OrderImprovement\|ProposeDisposition | samples/food-permit-ops/buc/buc_disposition.rdra:133 |
| condition | 意見陳述の手続を執る処分である | Disposition | Disposition.procedure in (聴聞, 弁明の機会の付与) | NotifyOpinionProcedure | samples/food-permit-ops/buc/buc_disposition.rdra:142 |
| condition | 聴聞を執る処分である | Disposition | Disposition.procedure == 聴聞 | DecideRevocation\|HoldHearing | samples/food-permit-ops/buc/buc_disposition.rdra:150 |
| condition | 弁明の機会を与える処分である | Disposition | Disposition.procedure == 弁明の機会の付与 | ReceiveExplanation | samples/food-permit-ops/buc/buc_disposition.rdra:159 |
| condition | 許可の取消である | Disposition | Disposition.kind == 許可の取消 | DecideRevocation | samples/food-permit-ops/buc/buc_disposition.rdra:176 |
| condition | 営業の停止である | Disposition | Disposition.kind == 営業の停止 | DecideSuspension | samples/food-permit-ops/buc/buc_disposition.rdra:187 |
| condition | 意見陳述を省く処分である | Disposition | Disposition.procedure == 意見陳述の省略（行政手続法13条2項3号） | OrderImprovement | samples/food-permit-ops/buc/buc_disposition.rdra:213 |
| condition | 処分が決定している | Disposition | Disposition.status in (処分決定, 確定（審査請求期間の経過）) | PublishViolation | samples/food-permit-ops/buc/buc_disposition.rdra:238 |
| forbidden | 許可の取消の前に聴聞を行う | Disposition | Disposition.kind == 許可の取消 AND Disposition.procedure != hearing |  | samples/food-permit-ops/buc/buc_disposition.rdra:361 |
| forbidden | 意見陳述を省けるのは技術的基準に従うよう命じる処分だけ | Disposition | Disposition.procedure == 意見陳述の省略（行政手続法13条2項3号） AND Disposition.kind != improvement_order |  | samples/food-permit-ops/buc/buc_disposition.rdra:363 |
| invariant | 不利益処分は理由を示して行う | Disposition | when Disposition.status in (decided, finalized) then Disposition.reason_shown == true |  | samples/food-permit-ops/buc/buc_disposition.rdra:365 |
| forbidden | 処分を決める前に公表しない | Disposition | Disposition.publication in (published, ended) AND Disposition.status not in (decided, finalized) |  | samples/food-permit-ops/buc/buc_disposition.rdra:367 |
| forbidden | 公表対象外は処分しないと決めたものだけ | Disposition | Disposition.publication == 公表対象外 AND Disposition.status != dropped |  | samples/food-permit-ops/buc/buc_disposition.rdra:369 |
| forbidden | 営業停止中は営業しない | BusinessPermit | BusinessPermit.status == 営業停止中 AND BusinessPermit.operating == true |  | samples/food-permit-ops/buc/buc_disposition.rdra:373 |
| forbidden | 許可なく営業しない（失効・取消・廃業の後の営業は無許可営業） | BusinessPermit | BusinessPermit.operating == true AND BusinessPermit.status in (expired, revoked, closed) |  | samples/food-permit-ops/buc/buc_disposition.rdra:377 |
| after | 停止を命じた許可では営業していない | BusinessPermit | BusinessPermit.status == 営業停止中 AND BusinessPermit.operating == false | SuspendBusiness | samples/food-permit-ops/buc/buc_disposition.rdra:379 |
| condition | 申請が検査待で、まだ検査の予定がない | PermitApplication\|Inspection | (PermitApplication.status == 検査待).none(Inspection.status in (検査予定, 適合, 不適合（改善の指示）)) | ScheduleInspection | samples/food-permit-ops/buc/buc_inspection_decision.rdra:120 |
| condition | 施設検査で施設基準に適合した | Inspection | Inspection.status == 適合 | GrantPermit\|RenewPermit | samples/food-permit-ops/buc/buc_inspection_decision.rdra:144 |
| condition | 新規の申請である | PermitApplication | PermitApplication.kind == 新規 | GrantPermit | samples/food-permit-ops/buc/buc_inspection_decision.rdra:148 |
| condition | 許可が有効である | BusinessPermit | BusinessPermit.status == 有効 | AppointHygieneManager\|IssueCertificate | samples/food-permit-ops/buc/buc_inspection_decision.rdra:173 |
| condition | 許可が有効で、許可証の交付を受けている | BusinessPermit | BusinessPermit.status == 有効, BusinessPermit.certificate_issued == true | StartBusiness | samples/food-permit-ops/buc/buc_inspection_decision.rdra:193 |
| forbidden | 施設基準に適合しない施設には許可しない | PermitApplication\|Inspection | PermitApplication.status == 許可 AND Inspection.status != 適合 |  | samples/food-permit-ops/buc/buc_inspection_decision.rdra:299 |
| invariant | 不許可には理由を同時に示す | PermitApplication | when PermitApplication.status == 不許可 then PermitApplication.reason_shown == true |  | samples/food-permit-ops/buc/buc_inspection_decision.rdra:301 |
| invariant | 手数料を納めた申請だけが検査と許可に進む | PermitApplication | when PermitApplication.status in (awaiting_inspection, approved) then PermitApplication.fee_paid == true |  | samples/food-permit-ops/buc/buc_inspection_decision.rdra:304 |
| invariant | 許可証の交付を受けてから営業する | BusinessPermit | when BusinessPermit.operating == true then BusinessPermit.certificate_issued == true |  | samples/food-permit-ops/buc/buc_inspection_decision.rdra:307 |
| condition | 監視指導計画が公表されている | MonitoringPlan | MonitoringPlan.status == 公表済 | ScheduleSiteVisit | samples/food-permit-ops/buc/buc_monitoring.rdra:84 |
| condition | 申請が確認待か差戻し中である | PermitApplication | PermitApplication.status in (申請内容確認待, 差戻し・補正中) | PayPermitFee | samples/food-permit-ops/buc/buc_permit_application.rdra:108 |
| condition | 手数料が納められている | PermitApplication | PermitApplication.fee_paid == true | ReviewApplication | samples/food-permit-ops/buc/buc_permit_application.rdra:117 |
| condition | 窓口（紙）で出した申請である | PermitApplication | PermitApplication.channel == 保健所の窓口（紙） | WithdrawApplication | samples/food-permit-ops/buc/buc_permit_application.rdra:140 |
| condition | 許可が終わっていない（有効か停止中） | BusinessPermit | BusinessPermit.status in (有効, 営業停止中) | FileChangeNotice\|FileSuccessionNotice | samples/food-permit-ops/buc/buc_permit_changes.rdra:82 |
| condition | 継続（更新）の申請である | PermitApplication | PermitApplication.kind == 継続（更新） | RenewPermit | samples/food-permit-ops/buc/buc_renewal.rdra:85 |
| after | 失効した許可は更新できない | BusinessPermit | BusinessPermit.status == 有効 | ExtendPermitTerm | samples/food-permit-ops/buc/buc_renewal.rdra:170 |

## 7. 非機能要件

| id | label | metric | target | window | slo | availability | resilience | audit | logging | retention | privacy | description |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| StandardProcessingPeriod | 標準処理期間（申請の到達から許可・不許可まで15開庁日） | processing_days | <=15 | 1 application | 98% |  |  |  |  |  |  |  |

| id | label | metric | target | window | slo | availability | resilience | audit | logging | retention | privacy | description |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| DispositionRecord | 処分の手続の記録 |  |  |  |  |  |  | enabled | structured | 10y |  |  |
| PermitLedgerAudit | 許可台帳と処分の記録 |  |  |  |  |  |  | enabled |  | 10y |  |  |
