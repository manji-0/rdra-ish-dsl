# Clinic Ops PRD

<!-- derived-from ../../docs/language-reference.md -->
<!-- derived-from ../../docs/cli-reference.md -->

`scripts/gen-clinic-ops-docs.py` が `samples/clinic-ops/` のモデルから CLI を通して生成する。図・表・説明文を手で直さず、`.rdra` を直して `scripts/check-sample-artifacts.sh` を実行する。題材は日本の病院の外来業務（受付、資格確認、診察、検査、処方、計算・会計、レセプト請求）で、入院は対象外。業務規則は `.rdra` の `//` コメントに一次資料の出典（ファイル名:行）を付けている。

この文書はプロダクトマネージャー向けである。なぜ作るか、誰のためか、何を満たせばよいかを示す。画面・API・データの設計は [設計書](design-doc.md) にある。

## 1. 背景と目的

成功指標の現状値と目標値はモデルが置いた想定の数字で、一次資料はない。実際の病院で使うときは、その病院の実績で置き換える。

### 診療の抜けと誤りを実施前に止める

処方やオーダの確認漏れ、診察後の連絡漏れは、患者の安全と継続した治療を損なう。

| 指標 | 現状 | 目標 | 期間 |
|---|---|---|---|
| 処方・オーダの確認漏れと診察後の連絡漏れの件数 | 月20件 | 月5件以下 | 月次 |

### 受付から診察までの待ち時間を短くする

受付後に診察室へ案内されるまでの待ちが長いと、患者が帰ってしまう。

| 指標 | 現状 | 目標 | 期間 |
|---|---|---|---|
| 受付から診察開始までの中央値 | 45分 | 30分 | 月次 |

### 会計から請求までの遅れと返戻を減らす

診察後の会計や請求に漏れや遅れがあると、収入が遅れ、返戻の訂正に事務の手間がかかる。

| 指標 | 現状 | 目標 | 期間 |
|---|---|---|---|
| 請求したレセプトのうち返戻になった割合 | 3% | 1% | 月次 |

## 2. 対象ユーザー

| 利用者 | 役割 |
|---|---|
| 医事課 会計担当 | 会計計算入力、患者の呼出、窓口精算、領収証・明細書の交付を行う事務職員。 |
| 患者支援・地域連携担当 | 診療後のケアプランを作り、患者への連絡と再診予約を追う。 |
| 医事課 レセプト担当 | レセプトの作成・院内点検・請求と、返戻・査定への対応を行う事務職員。 |
| 病院管理者 | 診療枠と診察室のマスタを管理し、監査の指摘を解決する。 |
| 医師 | 診察し、診断・オーダ・処方を出し、診療記録に署名する。 |
| 医師事務作業補助者 | 医師の指示で電子カルテへ代行入力する。確定は医師が行う。 |
| 看護師 | 医師の指示の下で診療の補助を行う。 |
| 患者 | 病院で診療を受ける人。 |
| 薬剤師 | 院内処方を調剤・監査し、服薬指導と交付を行う。疑わしい処方は医師に照会する。 |
| 医事課 受付担当 | 初診・再診の受付、資格確認、紹介状の受付、予約を行う事務職員。 |

### 誰がどの業務に関わるか

`list --kind buc-roles --format markdown` の出力である。`担当` は利用者がその BUC の UC を行うこと、`イベント送信` と `イベント受信` は外部システムがその BUC とイベントをやり取りすることを示す。

| role | kind | 診察 | 検体検査 | 処方 | 病院管理 | 予約 | フォローアップ | 新患登録・患者情報管理 | 受付（初診・再診） | レセプト作成・請求 | 計算・会計 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 医事課 会計担当 | actor |  |  |  |  |  |  |  |  |  | 担当 |
| 患者支援・地域連携担当 | actor |  |  |  |  |  | 担当 |  |  |  |  |
| 医事課 レセプト担当 | actor |  |  |  |  |  |  |  |  | 担当 |  |
| 病院管理者 | actor |  |  |  | 担当 |  |  |  |  |  |  |
| 医師 | actor | 担当 | 担当 | 担当 |  |  |  |  |  |  |  |
| 医師事務作業補助者 | actor |  | 担当 |  |  |  |  |  |  |  |  |
| 看護師 | actor | 担当 | 担当 |  |  |  |  |  | 担当 |  |  |
| 患者 | actor |  |  |  |  | 担当 |  | 担当 |  |  | 担当 |
| 薬剤師 | actor |  |  | 担当 |  |  |  |  |  |  |  |
| 医事課 受付担当 | actor |  |  |  |  | 担当 |  | 担当 | 担当 |  |  |
| 審査支払機関（支払基金・国保連） | extsystem |  |  |  |  |  |  |  |  | イベント送信、イベント受信 |  |
| 電子処方箋管理サービス | extsystem |  |  | イベント送信、イベント受信 |  |  |  |  |  |  |  |
| 保険薬局 | extsystem |  |  | イベント送信 |  |  |  |  |  |  |  |
| 患者通知サービス | extsystem |  |  |  |  | イベント受信 | イベント受信 |  |  |  |  |
| オンライン資格確認等システム | extsystem |  |  |  |  |  |  |  | イベント送信、イベント受信 |  |  |
| 外注検査システム | extsystem |  | イベント送信 |  |  |  |  |  |  |  |  |
| 自動精算機・クレジット決済 | extsystem |  |  |  |  |  |  |  |  |  | イベント送信、イベント受信 |

### システムコンテキスト図

```mermaid
%% rdra-ish diagram=context scope=whole model=samples/clinic-ops
%% kinds: actor extsystem system
%% omitted: buc step permission usecase event
flowchart LR
  actor__AccountsClerk(["医事課 会計担当"]):::actor
  %% desc actor__AccountsClerk: 会計計算入力、患者の呼出、窓口精算、領収証・明細書の交付を行う事務職員。
  actor__CareCoordinator(["患者支援・地域連携担当"]):::actor
  %% desc actor__CareCoordinator: 診療後のケアプランを作り、患者への連絡と再診予約を追う。
  actor__ClaimsClerk(["医事課 レセプト担当"]):::actor
  %% desc actor__ClaimsClerk: レセプトの作成・院内点検・請求と、返戻・査定への対応を行う事務職員。
  actor__ClinicAdmin(["病院管理者"]):::actor
  %% desc actor__ClinicAdmin: 診療枠と診察室のマスタを管理し、監査の指摘を解決する。
  actor__Clinician(["医師"]):::actor
  %% desc actor__Clinician: 診察し、診断・オーダ・処方を出し、診療記録に署名する。
  actor__MedicalClerk(["医師事務作業補助者"]):::actor
  %% desc actor__MedicalClerk: 医師の指示で電子カルテへ代行入力する。確定は医師が行う。
  actor__Nurse(["看護師"]):::actor
  %% desc actor__Nurse: 医師の指示の下で診療の補助を行う。
  actor__Patient(["患者"]):::actor
  %% desc actor__Patient: 病院で診療を受ける人。
  actor__Pharmacist(["薬剤師"]):::actor
  %% desc actor__Pharmacist: 院内処方を調剤・監査し、服薬指導と交付を行う。疑わしい処方は医師に照会する。
  actor__ReceptionClerk(["医事課 受付担当"]):::actor
  %% desc actor__ReceptionClerk: 初診・再診の受付、資格確認、紹介状の受付、予約を行う事務職員。
  extsystem__ClaimsReviewOrganization[["審査支払機関（支払基金・国保連）"]]:::extsystem
  extsystem__EPrescriptionService[["電子処方箋管理サービス"]]:::extsystem
  extsystem__InsurancePharmacy[["保険薬局"]]:::extsystem
  extsystem__MessagingGateway[["患者通知サービス"]]:::extsystem
  extsystem__OnlineEligibilitySystem[["オンライン資格確認等システム"]]:::extsystem
  extsystem__OutsourcedLabSystem[["外注検査システム"]]:::extsystem
  extsystem__PaymentGateway[["自動精算機・クレジット決済"]]:::extsystem
  system__ClinicOpsSystem["外来業務システム"]:::system
  actor__AccountsClerk -.->|計算・会計| system__ClinicOpsSystem
  %% derived actor__AccountsClerk -> system__ClinicOpsSystem via buc__BucVisitPayment
  actor__CareCoordinator -.->|フォローアップ| system__ClinicOpsSystem
  %% derived actor__CareCoordinator -> system__ClinicOpsSystem via buc__BucFollowupCare
  actor__ClaimsClerk -.->|レセプト作成・請求| system__ClinicOpsSystem
  %% derived actor__ClaimsClerk -> system__ClinicOpsSystem via buc__BucBillingClaims
  actor__ClinicAdmin -.->|病院管理| system__ClinicOpsSystem
  %% derived actor__ClinicAdmin -> system__ClinicOpsSystem via buc__BucStaffAdministration
  actor__Clinician -.->|診察、検体検査、処方| system__ClinicOpsSystem
  %% derived actor__Clinician -> system__ClinicOpsSystem via buc__BucClinicalEncounter,buc__BucOrdersResults,buc__BucPrescriptionFulfillment
  actor__MedicalClerk -.->|検体検査| system__ClinicOpsSystem
  %% derived actor__MedicalClerk -> system__ClinicOpsSystem via buc__BucOrdersResults
  actor__Nurse -.->|診察、検体検査、受付（初診・再診）| system__ClinicOpsSystem
  %% derived actor__Nurse -> system__ClinicOpsSystem via buc__BucClinicalEncounter,buc__BucOrdersResults,buc__BucVisitCheckIn
  actor__Patient -.->|予約、新患登録・患者情報管理、計算・会計| system__ClinicOpsSystem
  %% derived actor__Patient -> system__ClinicOpsSystem via buc__BucAppointmentScheduling,buc__BucPatientOnboarding,buc__BucVisitPayment
  actor__Pharmacist -.->|処方| system__ClinicOpsSystem
  %% derived actor__Pharmacist -> system__ClinicOpsSystem via buc__BucPrescriptionFulfillment
  actor__ReceptionClerk -.->|予約、新患登録・患者情報管理、受付（初診・再診）| system__ClinicOpsSystem
  %% derived actor__ReceptionClerk -> system__ClinicOpsSystem via buc__BucAppointmentScheduling,buc__BucPatientOnboarding,buc__BucVisitCheckIn
  extsystem__ClaimsReviewOrganization -.->|受付不能・要確認、レセプト受付、資格の振替・分割、返戻、査定、診療報酬の支払| system__ClinicOpsSystem
  %% derived extsystem__ClaimsReviewOrganization -> system__ClinicOpsSystem via event__EvClaimAccepted,event__EvClaimAssessed,event__EvClaimReceiptError,event__EvClaimReturned,event__EvClaimTransferred,event__EvRemittance
  extsystem__EPrescriptionService -.->|保険薬局での調剤完了| system__ClinicOpsSystem
  %% derived extsystem__EPrescriptionService -> system__ClinicOpsSystem via event__EvPrescriptionDispensed
  extsystem__InsurancePharmacy -.->|保険薬局からの疑義照会| system__ClinicOpsSystem
  %% derived extsystem__InsurancePharmacy -> system__ClinicOpsSystem via event__EvPharmacyInquiry
  extsystem__OnlineEligibilitySystem -.->|資格確認結果| system__ClinicOpsSystem
  %% derived extsystem__OnlineEligibilitySystem -> system__ClinicOpsSystem via event__EvEligibilityChecked
  extsystem__OutsourcedLabSystem -.->|検査結果の受信| system__ClinicOpsSystem
  %% derived extsystem__OutsourcedLabSystem -> system__ClinicOpsSystem via event__EvLabResultReceived
  extsystem__PaymentGateway -.->|自動精算機での精算完了| system__ClinicOpsSystem
  %% derived extsystem__PaymentGateway -> system__ClinicOpsSystem via event__EvPaymentSettled
  system__ClinicOpsSystem -.->|レセプト請求| extsystem__ClaimsReviewOrganization
  %% derived system__ClinicOpsSystem -> extsystem__ClaimsReviewOrganization via event__EvClaimStatementsSubmitted
  system__ClinicOpsSystem -.->|予約確定、予約変更、予約取消| extsystem__MessagingGateway
  %% derived system__ClinicOpsSystem -> extsystem__MessagingGateway via event__EvAppointmentCancelled,event__EvAppointmentRescheduled,event__EvAppointmentScheduled
  system__ClinicOpsSystem -.->|会計計算完了| extsystem__PaymentGateway
  %% derived system__ClinicOpsSystem -> extsystem__PaymentGateway via event__EvBillCalculated
  system__ClinicOpsSystem -.->|受付完了| extsystem__OnlineEligibilitySystem
  %% derived system__ClinicOpsSystem -> extsystem__OnlineEligibilitySystem via event__EvAppointmentCheckedIn
  system__ClinicOpsSystem -.->|電子処方箋の登録| extsystem__EPrescriptionService
  %% derived system__ClinicOpsSystem -> extsystem__EPrescriptionService via event__EvPrescriptionIssued
  classDef actor fill:#ffe3e3,stroke:#c92a2a,color:#000;
  classDef extsystem fill:#e9ecef,stroke:#495057,color:#000;
  classDef system fill:#c5f6fa,stroke:#0b7285,color:#000;
```

## 3. 要求

要求を目的ごとにまとめる。「BUC」はその要求が動機付ける業務で、[5. ユーザージャーニー](#5-ユーザージャーニー) にその業務フローがある。次の図は目的、要求、BUC のつながりである。「やらないこと」の枠にある要求は [4. やらないこと](#4-やらないこと) に載せている。

```mermaid
%% rdra-ish diagram=requirement scope=whole model=samples/clinic-ops
%% kinds: requirement goal buc
%% omitted: actor nfr quality constraint business flow usecase event system entity
flowchart LR
  requirement__ReqChangeAppointmentAtDesk[/"予約の変更・取消を電話と窓口で受け付ける"\]:::requirement
  requirement__ReqEligibilityEveryVisit[/"受付のたびに保険資格を確認する"\]:::requirement
  requirement__ReqFollowUpAfterVisit[/"診察後の連絡と再診予約をケアプランで追う"\]:::requirement
  requirement__ReqHandOffAfterConsultation[/"診察終了で受診を会計計算へ引き渡す"\]:::requirement
  requirement__ReqInquiryBeforeDispense[/"疑わしい処方は医師への照会が済むまで調剤しない"\]:::requirement
  requirement__ReqMonthlyClaimByTenth[/"前月分のレセプトを翌月10日までに請求する"\]:::requirement
  requirement__ReqPayAfterConsultation[/"会計は診察の後に一度で行う"\]:::requirement
  requirement__ReqProxyOrderNeedsPhysician[/"代行入力されたオーダは医師が確定するまで実施しない"\]:::requirement
  requirement__ReqRegisterEPrescription[/"院外処方箋を電子処方箋管理サービスへ登録し、保険薬局を指定しない"\]:::requirement
  requirement__ReqRegisterWithEligibility[/"新患登録で保険情報と限度額情報をオンライン資格確認から取り込む"\]:::requirement
  requirement__ReqResolveAuditFindings[/"監査で見つかった問題を管理者が解決するまで追う"\]:::requirement
  requirement__ReqResubmitReturned[/"返戻レセプトを訂正して再請求する"\]:::requirement
  requirement__ReqSwiftCheckIn[/"再来患者は情報を入力し直さずに受付・案内される"\]:::requirement
  goal__SafeCare[/"診療の抜けと誤りを実施前に止める<br/>処方・オーダの確認漏れと診察後の連絡漏れの件数: 月20件 → 月5件以下"/]:::goal
  %% desc goal__SafeCare: 処方やオーダの確認漏れ、診察後の連絡漏れは、患者の安全と継続した治療を損なう。
  goal__ShortWait[/"受付から診察までの待ち時間を短くする<br/>受付から診察開始までの中央値: 45分 → 30分"/]:::goal
  %% desc goal__ShortWait: 受付後に診察室へ案内されるまでの待ちが長いと、患者が帰ってしまう。
  goal__TimelyClaims[/"会計から請求までの遅れと返戻を減らす<br/>請求したレセプトのうち返戻になった割合: 3% → 1%"/]:::goal
  %% desc goal__TimelyClaims: 診察後の会計や請求に漏れや遅れがあると、収入が遅れ、返戻の訂正に事務の手間がかかる。
  buc__BucAppointmentScheduling["予約"]:::buc
  %% desc buc__BucAppointmentScheduling: 予約検索、仮押さえ、予約確定、変更、取消、通知を扱う。予約枠と予約レコードを同時に更新する UC は専用 API を持ち、予約通知は AppointmentNoticeApi に分ける。予約済みの患者は、来院前に `BucVisitCheckIn` の一括照会の対象になる。
  buc__BucBillingClaims["レセプト作成・請求"]:::buc
  %% desc buc__BucBillingClaims: レセプト（患者 × 診療月 × 保険者）の作成・院内点検・請求と、審査支払機関（支払基金・国保連）からの受付不能・返戻・査定・支払への対応を扱う。前月分は翌月10日までに請求し、受付不能・要確認は11日・12日に再送する。返戻は訂正して当月分と併せて再請求できるが、査定は再請求できず再審査請求で争う。資格が変わったレセプトは審査支払機関が振替・分割するので返戻にならないが、負担割合が異なる変更や公費の分割はできず返戻になる。支払は診療の翌々月。期間のうち DSL で表せないもの（10日、11・12日、5日〜月末、3か月、翌々月）はタイマーとコメントで示す。
  buc__BucClinicalEncounter["診察"]:::buc
  %% desc buc__BucClinicalEncounter: 診療記録を開き、バイタル、診断、署名、診察終了までを扱う。診察終了 `CompleteConsultation` で予約は「計算待ち」になり、会計は計算受付へ移る。他院紹介では医師が診療情報提供書を作成・送信する。検査オーダは診察中に医師が出し、署名の後に診察終了とする。署名イベント `EvEncounterSigned` はカルテのコスト情報の登録とケアプラン作成を、診察終了イベント `EvConsultationCompleted` は計算・会計を起動する。
  buc__BucFollowupCare["フォローアップ"]:::buc
  %% desc buc__BucFollowupCare: 診療後のケアプラン作成、患者連絡、患者応答確認、再診予約、ケアプラン終了を扱う。
  buc__BucOrdersResults["検体検査"]:::buc
  %% desc buc__BucOrdersResults: 検査オーダーから結果確認、重要結果通知までを扱う。医師事務作業補助者や看護師がオーダを代行入力した場合、医師が確定するまで看護師は採血しない（保助看法 第37条）。この規則は `OrderConfirmedByPhysician` 条件と `forbidden(ClinicalOrder, status == collected, physician_confirmed == false)` で検証する。外注検査の結果は外注検査システムから届く。採血管ラベルの発行と実施内容（コスト）の登録が会計計算の入力になる。実施入力のコストは `Charge.origin` が `performed` で、カルテ由来（`chart`）や選定療養費（`selectedfee`）のコストと発生元で区別される。
  buc__BucPatientOnboarding["新患登録・患者情報管理"]:::buc
  %% desc buc__BucPatientOnboarding: 新患登録から問診送付までを扱う。新患登録では、オンライン資格確認で得た保険情報と、患者が同意した場合の限度額情報（適用区分）を `InsurancePolicy` に登録する。負担割合 `burden_ratio` と限度額適用区分 `limit_category` は actor が選ぶ値ではなく保険者が決める値なので、variation 型の属性として持つ。
  buc__BucPrescriptionFulfillment["処方"]:::buc
  %% desc buc__BucPrescriptionFulfillment: 処方オーダから院外処方箋の発行、院内処方の調剤・監査・服薬指導・交付、疑義照会までを扱う。院外処方箋は電子処方箋管理サービスへ登録し、患者が選ぶ保険薬局で調剤される。病院は特定の保険薬局を指示しない（療担規則 第2条の5）ので、`Prescription` は薬局を持たない。使用期間は交付日を含めて4日以内で、日数は DSL で数えられないためタイマーで期限切れにする。薬剤師は疑わしい点があれば医師に問い合わせ、回答までは調剤しない。
  buc__BucStaffAdministration["病院管理"]:::buc
  %% desc buc__BucStaffAdministration: 病院運営のマスタ（診療枠・診察室）と監査の業務を扱う。
  buc__BucVisitCheckIn["受付（初診・再診）"]:::buc
  %% desc buc__BucVisitCheckIn: 到着確認、資格確認、紹介状の受付、診察室への案内を扱う。療養担当規則 第3条により受付のたびに資格を確認するので、`CheckInPatient` は必ずオンライン資格確認を伴い `EligibilityCheck` を作る。結果は 有効・無効・該当資格なし の 3 通りで、無効と該当資格なしは再確認へ戻る。システムが答えられない場合は被保険者資格申立書かシステム障害時モードで確認する。来院前には予約済み患者の一括照会を行う。受付では支払を受けない（会計は `BucVisitPayment`）。
  buc__BucVisitPayment["計算・会計"]:::buc
  %% desc buc__BucVisitPayment: 診察の後の会計計算入力、患者の呼出、会計窓口または自動精算機での精算、領収証・明細書の交付を扱う。一部負担金は負担割合で計算し、限度額情報があれば窓口での支払は自己負担限度額までになる（`CopayCappedAtLimit`）。紹介状なしの初診で除外に当たらない場合は選定療養費を自費の Charge として算定する（`UnreferredFirstVisit`）。金額は病院の設定 `SelectedTreatmentFeeSetting` に置き、モデルは数値を持たない。
  subgraph group_out_of_scope["やらないこと"]
    requirement__ReqNoOnlineConsultation[/"オンライン診療は対象外とする"\]:::requirement
  end
  goal__SafeCare --> requirement__ReqFollowUpAfterVisit
  %% derived goal__SafeCare -> requirement__ReqFollowUpAfterVisit via requirement__ReqFollowUpAfterVisit
  goal__SafeCare --> requirement__ReqInquiryBeforeDispense
  %% derived goal__SafeCare -> requirement__ReqInquiryBeforeDispense via requirement__ReqInquiryBeforeDispense
  goal__SafeCare --> requirement__ReqProxyOrderNeedsPhysician
  %% derived goal__SafeCare -> requirement__ReqProxyOrderNeedsPhysician via requirement__ReqProxyOrderNeedsPhysician
  goal__SafeCare --> requirement__ReqRegisterEPrescription
  %% derived goal__SafeCare -> requirement__ReqRegisterEPrescription via requirement__ReqRegisterEPrescription
  goal__ShortWait --> requirement__ReqChangeAppointmentAtDesk
  %% derived goal__ShortWait -> requirement__ReqChangeAppointmentAtDesk via requirement__ReqChangeAppointmentAtDesk
  goal__ShortWait --> requirement__ReqEligibilityEveryVisit
  %% derived goal__ShortWait -> requirement__ReqEligibilityEveryVisit via requirement__ReqEligibilityEveryVisit
  goal__ShortWait --> requirement__ReqRegisterWithEligibility
  %% derived goal__ShortWait -> requirement__ReqRegisterWithEligibility via requirement__ReqRegisterWithEligibility
  goal__ShortWait --> requirement__ReqSwiftCheckIn
  %% derived goal__ShortWait -> requirement__ReqSwiftCheckIn via requirement__ReqSwiftCheckIn
  goal__TimelyClaims --> requirement__ReqHandOffAfterConsultation
  %% derived goal__TimelyClaims -> requirement__ReqHandOffAfterConsultation via requirement__ReqHandOffAfterConsultation
  goal__TimelyClaims --> requirement__ReqMonthlyClaimByTenth
  %% derived goal__TimelyClaims -> requirement__ReqMonthlyClaimByTenth via requirement__ReqMonthlyClaimByTenth
  goal__TimelyClaims --> requirement__ReqPayAfterConsultation
  %% derived goal__TimelyClaims -> requirement__ReqPayAfterConsultation via requirement__ReqPayAfterConsultation
  goal__TimelyClaims --> requirement__ReqResolveAuditFindings
  %% derived goal__TimelyClaims -> requirement__ReqResolveAuditFindings via requirement__ReqResolveAuditFindings
  goal__TimelyClaims --> requirement__ReqResubmitReturned
  %% derived goal__TimelyClaims -> requirement__ReqResubmitReturned via requirement__ReqResubmitReturned
  requirement__ReqChangeAppointmentAtDesk --> buc__BucAppointmentScheduling
  requirement__ReqEligibilityEveryVisit --> buc__BucVisitCheckIn
  requirement__ReqFollowUpAfterVisit --> buc__BucFollowupCare
  requirement__ReqHandOffAfterConsultation --> buc__BucClinicalEncounter
  requirement__ReqInquiryBeforeDispense --> buc__BucPrescriptionFulfillment
  requirement__ReqMonthlyClaimByTenth --> buc__BucBillingClaims
  requirement__ReqNoOnlineConsultation -.-> buc__BucClinicalEncounter
  requirement__ReqPayAfterConsultation --> buc__BucVisitPayment
  requirement__ReqProxyOrderNeedsPhysician --> buc__BucOrdersResults
  requirement__ReqRegisterEPrescription --> buc__BucPrescriptionFulfillment
  requirement__ReqRegisterWithEligibility --> buc__BucPatientOnboarding
  requirement__ReqResolveAuditFindings --> buc__BucStaffAdministration
  requirement__ReqResubmitReturned --> buc__BucBillingClaims
  requirement__ReqSwiftCheckIn --> buc__BucVisitCheckIn
  classDef requirement fill:#fff3bf,stroke:#e67700,color:#000;
  classDef goal fill:#fff3bf,stroke:#e67700,color:#000;
  classDef buc fill:#d3f9d8,stroke:#2b8a3e,color:#000;
```

### 診療の抜けと誤りを実施前に止める

| ID | 要求 | 優先度 | 関係者 | 目的 | 受け入れ条件 | 出典 | BUC |
|---|---|---|---|---|---|---|---|
| `ReqFollowUpAfterVisit` | 診察後の連絡と再診予約をケアプランで追う | must | ケアコーディネーター | 診療の抜けと誤りを実施前に止める | 署名された受診ごとにケアプランが作られ、患者の応答と再診予約を経て終了する。 | モデルの想定（一次資料なし） | フォローアップ |
| `ReqInquiryBeforeDispense` | 疑わしい処方は医師への照会が済むまで調剤しない | must | 薬剤師 | 診療の抜けと誤りを実施前に止める | 疑義照会中の処方箋は、医師が回答するまで調剤・監査に進まない。 | 確かめた後でなければ調剤してはならない (egov-yakuzaishihou-335AC0000000146.txt:202) | 処方 |
| `ReqProxyOrderNeedsPhysician` | 代行入力されたオーダは医師が確定するまで実施しない | must | 看護師 | 診療の抜けと誤りを実施前に止める | 医師が確定していない代行入力オーダの検体は採取されない。 | 代行入力（代行オーダー） (mhlw-2020-dairi-nyuuryoku.txt:71-79, :15); 主治の医師又は歯科医師の指示 (egov-hokanhou-323AC0000000203.txt:311) | 検体検査 |
| `ReqRegisterEPrescription` | 院外処方箋を電子処方箋管理サービスへ登録し、保険薬局を指定しない | must | 医師 | 診療の抜けと誤りを実施前に止める | 院外処方箋は紙を印刷する場合も電子処方箋管理サービスへ登録され、処方箋は薬局を持たない。 | 電子処方箋管理サービス (mhlw-denshi-shohousen-unyou.txt:28-30); 特定の保険薬局において調剤を受けるべき旨の指示等を行つてはならない (egov-ryotan-kisoku-332M50000100015.txt:49) | 処方 |

### 受付から診察までの待ち時間を短くする

| ID | 要求 | 優先度 | 関係者 | 目的 | 受け入れ条件 | 出典 | BUC |
|---|---|---|---|---|---|---|---|
| `ReqChangeAppointmentAtDesk` | 予約の変更・取消を電話と窓口で受け付ける | must | 医事課 受付担当 | 受付から診察までの待ち時間を短くする | 受付担当が予約の変更・取消を入力すると、患者に通知が届く。 | sapporo-city-hosp-yoyaku.md:102, :118; yokohama-cu-urahp-c16010-shiyousyo.txt:196-197 | 予約 |
| `ReqEligibilityEveryVisit` | 受付のたびに保険資格を確認する | must | 医事課 受付担当 | 受付から診察までの待ち時間を短くする | 受付は必ずオンライン資格確認を伴い、確認できない場合は被保険者資格申立書かシステム障害時モードで確認する。 | 保険医療機関及び保険医療養担当規則 第3条 (egov-ryotan-kisoku-332M50000100015.txt:80) | 受付（初診・再診） |
| `ReqRegisterWithEligibility` | 新患登録で保険情報と限度額情報をオンライン資格確認から取り込む | must | 医事課 受付担当 | 受付から診察までの待ち時間を短くする | 保険情報は手入力せずオンライン資格確認の結果を登録し、限度額情報は患者が同意した場合だけ取得する。 | 新患登録 (mhlw-2024-hyojun-denshi-karte-gyomuflow.txt:46, :51); 新患登録 (koseikan-2024-iji-shiyousyo.txt:92) | 新患登録・患者情報管理 |
| `ReqSwiftCheckIn` | 再来患者は情報を入力し直さずに受付・案内される | must | 医事課 受付担当 | 受付から診察までの待ち時間を短くする | 再来患者は 1 回の確認操作で受付され、診察室へ案内される。 | モデルの想定（一次資料なし） | 受付（初診・再診） |

### 会計から請求までの遅れと返戻を減らす

| ID | 要求 | 優先度 | 関係者 | 目的 | 受け入れ条件 | 出典 | BUC |
|---|---|---|---|---|---|---|---|
| `ReqHandOffAfterConsultation` | 診察終了で受診を会計計算へ引き渡す | must | 医師 | 会計から請求までの遅れと返戻を減らす | 診察終了の後、予約は計算待ちになり、会計計算入力より前に完了しない。 | 診察終了後 (sendai-iji-gairai-shiyousyo.txt:176); 計算受付 (mhlw-2024-hyojun-denshi-karte-gyomuflow.txt:141) | 診察 |
| `ReqMonthlyClaimByTenth` | 前月分のレセプトを翌月10日までに請求する | must | 医事課 レセプト担当 | 会計から請求までの遅れと返戻を減らす | 前月分のレセプトは院内点検の後、毎月5日から10日の間に審査支払機関へ請求される。 | 各月分について翌月十日まで (egov-seikyu-meirei-351M50000100036.txt:55); 請求可能期間 5日～10日 (ssk-online-seikyu-sousa-tejunsho.txt:748) | レセプト作成・請求 |
| `ReqPayAfterConsultation` | 会計は診察の後に一度で行う | must | 医事課 会計担当 | 会計から請求までの遅れと返戻を減らす | 受付では支払を受けず、会計計算入力の後に窓口または自動精算機で精算し、領収証を交付する。 | 標準型電子カルテ 業務フロー: 計算受付・会計受付 (mhlw-2024-hyojun-denshi-karte-gyomuflow.txt:141-144) | 計算・会計 |
| `ReqResolveAuditFindings` | 監査で見つかった問題を管理者が解決するまで追う | must | 病院管理者 | 会計から請求までの遅れと返戻を減らす | 監査指摘は管理端末から解決されるまで未解決として残る。 | モデルの想定（一次資料なし） | 病院管理 |
| `ReqResubmitReturned` | 返戻レセプトを訂正して再請求する | must | 医事課 レセプト担当 | 会計から請求までの遅れと返戻を減らす | 返戻は3か月以内に取り込まれ、訂正して当月分と併せて再請求される。査定は再請求せず再審査請求で争う。 | 返戻ファイルを修正し、当月請求のレセプトと併せて再請求 (mhlw-online-henrei-saiseikyu.txt:105-111, :119) | レセプト作成・請求 |

## 4. やらないこと

| ID | 内容 | 理由 |
|---|---|---|
| `ReqNoOnlineConsultation` | オンライン診療は対象外とする | 診察は来院して受ける前提で業務を分けているため、ビデオ通話による受付・診察・会計は扱わない。 |

## 5. ユーザージャーニー

次の図は業務の流れである。業務ごとに BUC をまとめ、ある BUC から次の BUC への引き渡しを、引き渡すイベントのラベルで示す。

```mermaid
%% rdra-ish diagram=business scope=whole model=samples/clinic-ops
%% kinds: business buc
%% omitted: actor requirement flow step timer medium permission usecase screen event api entity condition
flowchart LR
  subgraph business__CareDelivery["診療"]
    buc__BucClinicalEncounter["診察"]:::buc
    %% desc buc__BucClinicalEncounter: 診療記録を開き、バイタル、診断、署名、診察終了までを扱う。診察終了 `CompleteConsultation` で予約は「計算待ち」になり、会計は計算受付へ移る。他院紹介では医師が診療情報提供書を作成・送信する。検査オーダは診察中に医師が出し、署名の後に診察終了とする。署名イベント `EvEncounterSigned` はカルテのコスト情報の登録とケアプラン作成を、診察終了イベント `EvConsultationCompleted` は計算・会計を起動する。
    buc__BucOrdersResults["検体検査"]:::buc
    %% desc buc__BucOrdersResults: 検査オーダーから結果確認、重要結果通知までを扱う。医師事務作業補助者や看護師がオーダを代行入力した場合、医師が確定するまで看護師は採血しない（保助看法 第37条）。この規則は `OrderConfirmedByPhysician` 条件と `forbidden(ClinicalOrder, status == collected, physician_confirmed == false)` で検証する。外注検査の結果は外注検査システムから届く。採血管ラベルの発行と実施内容（コスト）の登録が会計計算の入力になる。実施入力のコストは `Charge.origin` が `performed` で、カルテ由来（`chart`）や選定療養費（`selectedfee`）のコストと発生元で区別される。
    buc__BucPrescriptionFulfillment["処方"]:::buc
    %% desc buc__BucPrescriptionFulfillment: 処方オーダから院外処方箋の発行、院内処方の調剤・監査・服薬指導・交付、疑義照会までを扱う。院外処方箋は電子処方箋管理サービスへ登録し、患者が選ぶ保険薬局で調剤される。病院は特定の保険薬局を指示しない（療担規則 第2条の5）ので、`Prescription` は薬局を持たない。使用期間は交付日を含めて4日以内で、日数は DSL で数えられないためタイマーで期限切れにする。薬剤師は疑わしい点があれば医師に問い合わせ、回答までは調剤しない。
  end
  subgraph business__ClinicAdministration["病院管理"]
    buc__BucStaffAdministration["病院管理"]:::buc
    %% desc buc__BucStaffAdministration: 病院運営のマスタ（診療枠・診察室）と監査の業務を扱う。
  end
  subgraph business__PatientAccess["患者受付"]
    %% desc business__PatientAccess: 新患登録から予約、受付、フォローアップまでの患者に向き合う業務。
    buc__BucAppointmentScheduling["予約"]:::buc
    %% desc buc__BucAppointmentScheduling: 予約検索、仮押さえ、予約確定、変更、取消、通知を扱う。予約枠と予約レコードを同時に更新する UC は専用 API を持ち、予約通知は AppointmentNoticeApi に分ける。予約済みの患者は、来院前に `BucVisitCheckIn` の一括照会の対象になる。
    buc__BucFollowupCare["フォローアップ"]:::buc
    %% desc buc__BucFollowupCare: 診療後のケアプラン作成、患者連絡、患者応答確認、再診予約、ケアプラン終了を扱う。
    buc__BucPatientOnboarding["新患登録・患者情報管理"]:::buc
    %% desc buc__BucPatientOnboarding: 新患登録から問診送付までを扱う。新患登録では、オンライン資格確認で得た保険情報と、患者が同意した場合の限度額情報（適用区分）を `InsurancePolicy` に登録する。負担割合 `burden_ratio` と限度額適用区分 `limit_category` は actor が選ぶ値ではなく保険者が決める値なので、variation 型の属性として持つ。
    buc__BucVisitCheckIn["受付（初診・再診）"]:::buc
    %% desc buc__BucVisitCheckIn: 到着確認、資格確認、紹介状の受付、診察室への案内を扱う。療養担当規則 第3条により受付のたびに資格を確認するので、`CheckInPatient` は必ずオンライン資格確認を伴い `EligibilityCheck` を作る。結果は 有効・無効・該当資格なし の 3 通りで、無効と該当資格なしは再確認へ戻る。システムが答えられない場合は被保険者資格申立書かシステム障害時モードで確認する。来院前には予約済み患者の一括照会を行う。受付では支払を受けない（会計は `BucVisitPayment`）。
  end
  subgraph business__RevenueCycle["医事会計"]
    buc__BucBillingClaims["レセプト作成・請求"]:::buc
    %% desc buc__BucBillingClaims: レセプト（患者 × 診療月 × 保険者）の作成・院内点検・請求と、審査支払機関（支払基金・国保連）からの受付不能・返戻・査定・支払への対応を扱う。前月分は翌月10日までに請求し、受付不能・要確認は11日・12日に再送する。返戻は訂正して当月分と併せて再請求できるが、査定は再請求できず再審査請求で争う。資格が変わったレセプトは審査支払機関が振替・分割するので返戻にならないが、負担割合が異なる変更や公費の分割はできず返戻になる。支払は診療の翌々月。期間のうち DSL で表せないもの（10日、11・12日、5日〜月末、3か月、翌々月）はタイマーとコメントで示す。
    buc__BucVisitPayment["計算・会計"]:::buc
    %% desc buc__BucVisitPayment: 診察の後の会計計算入力、患者の呼出、会計窓口または自動精算機での精算、領収証・明細書の交付を扱う。一部負担金は負担割合で計算し、限度額情報があれば窓口での支払は自己負担限度額までになる（`CopayCappedAtLimit`）。紹介状なしの初診で除外に当たらない場合は選定療養費を自費の Charge として算定する（`UnreferredFirstVisit`）。金額は病院の設定 `SelectedTreatmentFeeSetting` に置き、モデルは数値を持たない。
  end
  buc__BucAppointmentScheduling -->|予約確定| buc__BucPatientOnboarding
  %% derived buc__BucAppointmentScheduling -> buc__BucPatientOnboarding via event__EvAppointmentScheduled
  buc__BucClinicalEncounter -->|診察終了| buc__BucVisitPayment
  %% derived buc__BucClinicalEncounter -> buc__BucVisitPayment via event__EvConsultationCompleted
  buc__BucClinicalEncounter -->|診療記録の署名| buc__BucBillingClaims
  %% derived buc__BucClinicalEncounter -> buc__BucBillingClaims via event__EvEncounterSigned
  buc__BucClinicalEncounter -->|診療記録の署名| buc__BucFollowupCare
  %% derived buc__BucClinicalEncounter -> buc__BucFollowupCare via event__EvEncounterSigned
  buc__BucFollowupCare -->|予約確定| buc__BucAppointmentScheduling
  %% derived buc__BucFollowupCare -> buc__BucAppointmentScheduling via event__EvAppointmentScheduled
  buc__BucFollowupCare -->|予約確定| buc__BucPatientOnboarding
  %% derived buc__BucFollowupCare -> buc__BucPatientOnboarding via event__EvAppointmentScheduled
  buc__BucOrdersResults -->|検査結果の確認| buc__BucFollowupCare
  %% derived buc__BucOrdersResults -> buc__BucFollowupCare via event__EvClinicalOrderReviewed
  buc__BucVisitCheckIn -->|受付完了| buc__BucClinicalEncounter
  %% derived buc__BucVisitCheckIn -> buc__BucClinicalEncounter via event__EvAppointmentCheckedIn
  classDef business fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef buc fill:#d3f9d8,stroke:#2b8a3e,color:#000;
```

この後の節は BUC ごとの業務フローである。手順、分岐、例外と、各手順を担う UC を示す。業務フローを持たない BUC（病院管理）は載せていない。

### 新患登録・患者情報管理

```mermaid
%% rdra-ish diagram=flow scope=buc:BucPatientOnboarding model=samples/clinic-ops
%% kinds: flow step lane timer usecase event
%% omitted: buc
flowchart TD
  subgraph flow__OnboardingFlow["新患登録の流れ"]
    step__IntakeExpireStep["問診票の期限切れ"]:::step
    subgraph lane__OnboardingFlow__Patient["患者"]
      %% lane lane__OnboardingFlow__Patient: actor__Patient
      step__ConsentStep["同意の取得"]:::step
      step__IntakeAnswerStep["問診票の回答"]:::step
    end
    subgraph lane__OnboardingFlow__ReceptionClerk["医事課 受付担当"]
      %% lane lane__OnboardingFlow__ReceptionClerk: actor__ReceptionClerk
      step__InsuranceStep["保険資格・限度額情報の登録"]:::step
      step__IntakeSendStep["問診票の送付"]:::step
      step__NewPatientSearchStep["既存患者の検索"]:::step
      step__RegisterStep["新患登録"]:::step
    end
  end
  subgraph flow__PatientMaintenanceFlow["患者情報の保守"]
    subgraph lane__PatientMaintenanceFlow__ReceptionClerk["医事課 受付担当"]
      %% lane lane__PatientMaintenanceFlow__ReceptionClerk: actor__ReceptionClerk
      step__ArchiveStep["患者のアーカイブ"]:::step
      step__DemographicsStep["患者属性の変更"]:::step
      step__MaintainSearchStep["登録済み患者の検索"]:::step
      step__MergeStep["重複患者の統合"]:::step
    end
  end
  timer__IntakeExpiryCheck(("問診票の期限切れ確認（毎晩）")):::timer
  usecase__ArchivePatient("患者のアーカイブ"):::usecase
  %% desc usecase__ArchivePatient: 患者アーカイブ状態。
  usecase__CaptureConsent("同意の取得"):::usecase
  %% desc usecase__CaptureConsent: 同意記録を作る。同意の有無は Bool、未回答は null で持つ。
  usecase__CompleteIntakeForms("問診票の回答"):::usecase
  %% desc usecase__CompleteIntakeForms: 問診完了イベント。
  usecase__ExpireIntakeForms("問診票の期限切れ"):::usecase
  %% desc usecase__ExpireIntakeForms: 問診期限切れイベント。
  usecase__MergeDuplicatePatient("重複患者の統合"):::usecase
  %% desc usecase__MergeDuplicatePatient: 患者統合状態。
  usecase__RegisterPatient("新患登録"):::usecase
  %% desc usecase__RegisterPatient: 新患の PatientAccount と PatientProfile を 1 つの API で同時に作る。
  usecase__SearchPatient("患者の検索"):::usecase
  %% desc usecase__SearchPatient: 患者を検索する。参照だけの API で、患者の作成とは分ける。
  usecase__SendIntakeForms("問診票の送付"):::usecase
  %% desc usecase__SendIntakeForms: 問診パケットとメッセージ作成。
  usecase__UpdateDemographics("患者属性の変更"):::usecase
  %% desc usecase__UpdateDemographics: 患者の属性を API で更新する。
  usecase__VerifyInsurance("保険資格・限度額情報の登録（新患登録）"):::usecase
  %% desc usecase__VerifyInsurance: 保険資格の登録と限度額情報（適用区分）の取得。
  event__EvAppointmentScheduled>"予約確定"]:::event
  step__ArchiveStep -.-> usecase__ArchivePatient
  step__ConsentStep -.-> usecase__CaptureConsent
  step__ConsentStep -->|precedes| step__IntakeSendStep
  step__DemographicsStep -.-> usecase__UpdateDemographics
  step__InsuranceStep -.-> usecase__VerifyInsurance
  step__InsuranceStep -->|precedes| step__ConsentStep
  step__IntakeAnswerStep -.-> usecase__CompleteIntakeForms
  step__IntakeAnswerStep -.->|excepts| step__IntakeExpireStep
  step__IntakeExpireStep -.-> timer__IntakeExpiryCheck
  step__IntakeExpireStep -.-> usecase__ExpireIntakeForms
  step__IntakeSendStep -.-> event__EvAppointmentScheduled
  step__IntakeSendStep -.-> usecase__SendIntakeForms
  step__IntakeSendStep -->|precedes| step__IntakeAnswerStep
  step__MaintainSearchStep -->|branches| step__ArchiveStep
  step__MaintainSearchStep -->|branches| step__MergeStep
  step__MaintainSearchStep -.-> usecase__SearchPatient
  step__MaintainSearchStep -->|precedes| step__DemographicsStep
  step__MergeStep -.-> usecase__MergeDuplicatePatient
  step__NewPatientSearchStep -.-> usecase__SearchPatient
  step__NewPatientSearchStep -->|precedes| step__RegisterStep
  step__RegisterStep -.-> usecase__RegisterPatient
  step__RegisterStep -->|precedes| step__InsuranceStep
  classDef flow fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef step fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef lane fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef timer fill:#ffe8cc,stroke:#d9480f,color:#000;
  classDef usecase fill:#d0ebff,stroke:#1864ab,color:#000;
  classDef event fill:#ffdeeb,stroke:#a61e4d,color:#000;
```

### 予約

```mermaid
%% rdra-ish diagram=flow scope=buc:BucAppointmentScheduling model=samples/clinic-ops
%% kinds: flow step lane actor timer usecase event
%% omitted: buc
flowchart TD
  subgraph flow__AppointmentFlow["予約の流れ"]
    step__AvailabilityStep["空き枠の検索"]:::step
    step__NoShowStep["未来院の処理"]:::step
    subgraph lane__AppointmentFlow__ReceptionClerk["医事課 受付担当"]
      %% lane lane__AppointmentFlow__ReceptionClerk: actor__ReceptionClerk
      step__BookStep["予約確定"]:::step
      step__CancelStep["予約の取消"]:::step
      step__NoticeStep["予約の通知"]:::step
      step__RescheduleStep["予約の変更"]:::step
      step__ReserveStep["仮押さえ"]:::step
    end
  end
  actor__Patient(["患者"]):::actor
  %% desc actor__Patient: 病院で診療を受ける人。
  actor__ReceptionClerk(["医事課 受付担当"]):::actor
  %% desc actor__ReceptionClerk: 初診・再診の受付、資格確認、紹介状の受付、予約を行う事務職員。
  timer__NoShowSweep(("未来院の確認（毎晩）")):::timer
  usecase__BookAppointment("予約の確定"):::usecase
  %% desc usecase__BookAppointment: 予約確定と予約枠更新。
  usecase__CancelAppointment("予約の取消"):::usecase
  %% desc usecase__CancelAppointment: 取消理由と予約枠更新。
  usecase__MarkNoShow("未来院の処理"):::usecase
  %% desc usecase__MarkNoShow: タイマー起動の未来院処理。
  usecase__RescheduleAppointment("予約の変更"):::usecase
  %% desc usecase__RescheduleAppointment: 予約を変更する。予約の状態は変わらない。
  usecase__ReserveAppointment("予約の仮押さえ"):::usecase
  %% desc usecase__ReserveAppointment: 仮押さえで Appointment を作る。
  usecase__SearchAvailability("空き枠の検索"):::usecase
  %% desc usecase__SearchAvailability: 予約枠を検索する。参照だけの API。
  usecase__SendAppointmentNotice("予約の通知"):::usecase
  %% desc usecase__SendAppointmentNotice: 予約イベントからの通知作成。
  event__EvAppointmentCancelled>"予約取消"]:::event
  actor__Patient -.-> step__AvailabilityStep
  actor__ReceptionClerk -.-> step__AvailabilityStep
  step__AvailabilityStep -.-> usecase__SearchAvailability
  step__AvailabilityStep -->|precedes| step__ReserveStep
  step__BookStep -.-> usecase__BookAppointment
  step__BookStep -->|precedes| step__NoticeStep
  step__CancelStep -.-> event__EvAppointmentCancelled
  step__CancelStep -.-> usecase__CancelAppointment
  step__NoShowStep -.-> timer__NoShowSweep
  step__NoShowStep -.-> usecase__MarkNoShow
  step__NoticeStep -->|branches| step__CancelStep
  step__NoticeStep -->|branches| step__RescheduleStep
  step__NoticeStep -.-> usecase__SendAppointmentNotice
  step__NoticeStep -.->|excepts| step__NoShowStep
  step__RescheduleStep -.-> usecase__RescheduleAppointment
  step__RescheduleStep -->|repeats| step__NoticeStep
  step__ReserveStep -.-> usecase__ReserveAppointment
  step__ReserveStep -->|precedes| step__BookStep
  classDef flow fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef step fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef lane fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef actor fill:#ffe3e3,stroke:#c92a2a,color:#000;
  classDef timer fill:#ffe8cc,stroke:#d9480f,color:#000;
  classDef usecase fill:#d0ebff,stroke:#1864ab,color:#000;
  classDef event fill:#ffdeeb,stroke:#a61e4d,color:#000;
```

### 受付（初診・再診）

```mermaid
%% rdra-ish diagram=flow scope=buc:BucVisitCheckIn model=samples/clinic-ops
%% kinds: flow step lane usecase
%% omitted: buc
flowchart TD
  subgraph flow__VisitCheckInFlow["受付の流れ"]
    subgraph lane__VisitCheckInFlow__Nurse["看護師"]
      %% lane lane__VisitCheckInFlow__Nurse: actor__Nurse
      step__RoomingStep["診察室への案内"]:::step
    end
    subgraph lane__VisitCheckInFlow__ReceptionClerk["医事課 受付担当"]
      %% lane lane__VisitCheckInFlow__ReceptionClerk: actor__ReceptionClerk
      step__ArrivalStep["到着・予約票の確認"]:::step
      step__DeclarationStep["資格を確認できない（申立書・障害時モード）"]:::step
      step__EligibilityStep["資格確認"]:::step
      step__ReferralStep["紹介状あり（初診）"]:::step
    end
  end
  usecase__AssignRoom("診察室への案内"):::usecase
  %% desc usecase__AssignRoom: 受付済の受診だけを診察室へ案内する。予約の状態は変わらない。
  usecase__CheckInPatient("受付（初診・再診）"):::usecase
  %% desc usecase__CheckInPatient: 受付で予約を受付済にし、同時にオンライン資格確認を作る。
  usecase__ConfirmEligibilityInOutageMode("システム障害時モードでの資格確認"):::usecase
  %% desc usecase__ConfirmEligibilityInOutageMode: システム障害時モードによる確認。
  usecase__ReceiveEligibilityInvalid("資格確認結果（無効）の取込"):::usecase
  %% desc usecase__ReceiveEligibilityInvalid: 資格確認結果「無効」の取込。
  usecase__ReceiveEligibilityNoInfo("資格確認結果（該当資格なし）の取込"):::usecase
  %% desc usecase__ReceiveEligibilityNoInfo: 資格確認結果「該当資格なし」の取込。
  usecase__ReceiveEligibilityValid("資格確認結果（有効）の取込"):::usecase
  %% desc usecase__ReceiveEligibilityValid: 資格確認結果「有効」の取込。
  usecase__ReceiveReferralLetter("紹介状受付処理"):::usecase
  %% desc usecase__ReceiveReferralLetter: 紹介状の受領と受付処理。
  usecase__RecheckEligibility("資格の再確認"):::usecase
  %% desc usecase__RecheckEligibility: 無効・該当資格なしからの再確認。
  usecase__RecordEligibilityDeclaration("被保険者資格申立書の受領"):::usecase
  %% desc usecase__RecordEligibilityDeclaration: 被保険者資格申立書による確認。
  usecase__VerifyArrival("予約票・診察券の確認"):::usecase
  %% desc usecase__VerifyArrival: 予約票・診察券を確認する（参照のみ）。
  step__ArrivalStep -.-> usecase__VerifyArrival
  step__ArrivalStep -->|precedes| step__EligibilityStep
  step__DeclarationStep -.-> usecase__ConfirmEligibilityInOutageMode
  step__DeclarationStep -.-> usecase__RecordEligibilityDeclaration
  step__DeclarationStep -->|precedes| step__RoomingStep
  step__EligibilityStep -->|branches| step__ReferralStep
  step__EligibilityStep -.-> usecase__CheckInPatient
  step__EligibilityStep -.-> usecase__ReceiveEligibilityInvalid
  step__EligibilityStep -.-> usecase__ReceiveEligibilityNoInfo
  step__EligibilityStep -.-> usecase__ReceiveEligibilityValid
  step__EligibilityStep -.-> usecase__RecheckEligibility
  step__EligibilityStep -.->|excepts| step__DeclarationStep
  step__EligibilityStep -->|precedes| step__RoomingStep
  step__ReferralStep -.-> usecase__ReceiveReferralLetter
  step__ReferralStep -->|precedes| step__RoomingStep
  step__RoomingStep -.-> usecase__AssignRoom
  classDef flow fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef step fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef lane fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef usecase fill:#d0ebff,stroke:#1864ab,color:#000;
```

### 診察

```mermaid
%% rdra-ish diagram=flow scope=buc:BucClinicalEncounter model=samples/clinic-ops
%% kinds: flow step lane usecase
%% omitted: buc
flowchart TD
  subgraph flow__EncounterFlow["診察の流れ"]
    subgraph lane__EncounterFlow__Clinician["医師"]
      %% lane lane__EncounterFlow__Clinician: actor__Clinician
      step__AmendStep["署名後の修正"]:::step
      step__AssessmentStep["診察・診断の記録"]:::step
      step__CompleteStep["診察終了"]:::step
      step__ReferralOutStep["診療情報提供書の作成・送信"]:::step
      step__SignStep["診療記録の署名"]:::step
    end
    subgraph lane__EncounterFlow__Nurse["看護師"]
      %% lane lane__EncounterFlow__Nurse: actor__Nurse
      step__OpenStep["診療記録を開く"]:::step
      step__VitalsStep["バイタル測定"]:::step
    end
  end
  usecase__AmendEncounter("署名済み記録の修正"):::usecase
  %% desc usecase__AmendEncounter: 署名済み記録の修正。
  usecase__CompleteConsultation("診察終了"):::usecase
  %% desc usecase__CompleteConsultation: 診察終了で予約を計算待ちにし、部屋を清掃中にし、会計を起動する。
  usecase__DocumentAssessment("診察・診断の記録"):::usecase
  %% desc usecase__DocumentAssessment: 診断記録と記録状態。
  usecase__IssueReferralLetter("診療情報提供書の作成・送信"):::usecase
  %% desc usecase__IssueReferralLetter: 診療情報提供書の作成・送信。
  usecase__OpenEncounter("診療記録を開く"):::usecase
  %% desc usecase__OpenEncounter: 受付済の予約から診療記録を開く。
  usecase__RecordVitals("バイタルの記録"):::usecase
  %% desc usecase__RecordVitals: バイタル記録。
  usecase__SignEncounter("診療記録の署名"):::usecase
  %% desc usecase__SignEncounter: 署名でコスト情報登録とケアプラン作成を起動する。
  step__AmendStep -.-> usecase__AmendEncounter
  step__AssessmentStep -->|branches| step__ReferralOutStep
  step__AssessmentStep -.-> usecase__DocumentAssessment
  step__AssessmentStep -->|precedes| step__SignStep
  step__CompleteStep -.-> usecase__CompleteConsultation
  step__OpenStep -.-> usecase__OpenEncounter
  step__OpenStep -->|precedes| step__VitalsStep
  step__ReferralOutStep -.-> usecase__IssueReferralLetter
  step__ReferralOutStep -->|precedes| step__SignStep
  step__SignStep -.-> usecase__SignEncounter
  step__SignStep -.->|excepts| step__AmendStep
  step__SignStep -->|precedes| step__CompleteStep
  step__VitalsStep -.-> usecase__RecordVitals
  step__VitalsStep -->|precedes| step__AssessmentStep
  classDef flow fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef step fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef lane fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef usecase fill:#d0ebff,stroke:#1864ab,color:#000;
```

### 検体検査

```mermaid
%% rdra-ish diagram=flow scope=buc:BucOrdersResults model=samples/clinic-ops
%% kinds: flow step lane actor usecase event
%% omitted: buc
flowchart TD
  subgraph flow__LabOrderFlow["検体検査の流れ"]
    step__ProxyOrderStep["オーダの代行入力"]:::step
    subgraph lane__LabOrderFlow__Clinician["医師"]
      %% lane lane__LabOrderFlow__Clinician: actor__Clinician
      step__ConfirmOrderStep["代行入力オーダの確定"]:::step
      step__CriticalStep["重要結果の通知"]:::step
      step__LabOrderStep["検査オーダ"]:::step
      step__OrderCancelStep["オーダの取消"]:::step
      step__ResultReviewStep["検査結果の確認"]:::step
    end
    subgraph lane__LabOrderFlow__Nurse["看護師"]
      %% lane lane__LabOrderFlow__Nurse: actor__Nurse
      step__CollectStep["採血"]:::step
      step__CostStep["実施内容（コスト）の登録"]:::step
      step__LabResultStep["検査結果の受信"]:::step
      step__LabelStep["採血管ラベルの発行"]:::step
    end
  end
  actor__MedicalClerk(["医師事務作業補助者"]):::actor
  %% desc actor__MedicalClerk: 医師の指示で電子カルテへ代行入力する。確定は医師が行う。
  actor__Nurse(["看護師"]):::actor
  %% desc actor__Nurse: 医師の指示の下で診療の補助を行う。
  usecase__CancelClinicalOrder("オーダの取消"):::usecase
  %% desc usecase__CancelClinicalOrder: オーダ取消。
  usecase__CollectSpecimen("採血"):::usecase
  %% desc usecase__CollectSpecimen: 医師が確定したオーダだけを採取する。
  usecase__ConfirmProxyOrder("代行入力オーダの確定"):::usecase
  %% desc usecase__ConfirmProxyOrder: 医師による代行入力オーダの確定。
  usecase__EnterProxyOrder("検体検査オーダの代行入力"):::usecase
  %% desc usecase__EnterProxyOrder: 代行入力オーダは未確定で作られる。
  usecase__NotifyCriticalResult("重要結果（パニック値）の通知"):::usecase
  %% desc usecase__NotifyCriticalResult: 重要結果の患者通知。
  usecase__PlaceLabOrder("検体検査オーダの登録"):::usecase
  %% desc usecase__PlaceLabOrder: 医師が登録したオーダは確定済み。
  usecase__PrintSpecimenLabel("採血管ラベル発行・貼付"):::usecase
  %% desc usecase__PrintSpecimenLabel: 採血管ラベルの発行（参照のみ）。
  usecase__ReceiveLabResult("検査結果の受信"):::usecase
  %% desc usecase__ReceiveLabResult: 外注検査システムからの結果受信。
  usecase__RegisterPerformedProcedure("実施内容（コスト）の登録"):::usecase
  %% desc usecase__RegisterPerformedProcedure: 実施内容（コスト）の登録。
  usecase__ReviewLabResult("検査結果の確認"):::usecase
  %% desc usecase__ReviewLabResult: 結果確認と後続通知の起点。
  event__EvLabResultReceived>"検査結果の受信"]:::event
  actor__MedicalClerk -.-> step__ProxyOrderStep
  actor__Nurse -.-> step__ProxyOrderStep
  step__CollectStep -.-> usecase__CollectSpecimen
  step__CollectStep -->|precedes| step__CostStep
  step__ConfirmOrderStep -.-> usecase__ConfirmProxyOrder
  step__ConfirmOrderStep -->|precedes| step__LabelStep
  step__CostStep -.-> usecase__RegisterPerformedProcedure
  step__CostStep -->|precedes| step__LabResultStep
  step__CriticalStep -.-> usecase__NotifyCriticalResult
  step__LabOrderStep -->|branches| step__ProxyOrderStep
  step__LabOrderStep -.-> usecase__PlaceLabOrder
  step__LabOrderStep -.->|excepts| step__OrderCancelStep
  step__LabOrderStep -->|precedes| step__LabelStep
  step__LabResultStep -.-> event__EvLabResultReceived
  step__LabResultStep -.-> usecase__ReceiveLabResult
  step__LabResultStep -->|precedes| step__ResultReviewStep
  step__LabelStep -.-> usecase__PrintSpecimenLabel
  step__LabelStep -->|precedes| step__CollectStep
  step__OrderCancelStep -.-> usecase__CancelClinicalOrder
  step__ProxyOrderStep -.-> usecase__EnterProxyOrder
  step__ProxyOrderStep -->|precedes| step__ConfirmOrderStep
  step__ResultReviewStep -->|branches| step__CriticalStep
  step__ResultReviewStep -.-> usecase__ReviewLabResult
  classDef flow fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef step fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef lane fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef actor fill:#ffe3e3,stroke:#c92a2a,color:#000;
  classDef usecase fill:#d0ebff,stroke:#1864ab,color:#000;
  classDef event fill:#ffdeeb,stroke:#a61e4d,color:#000;
```

### 処方

```mermaid
%% rdra-ish diagram=flow scope=buc:BucPrescriptionFulfillment model=samples/clinic-ops
%% kinds: flow step lane actor usecase
%% omitted: buc
flowchart TD
  subgraph flow__PrescriptionFlow["処方の流れ"]
    step__InquiryStep["疑義照会"]:::step
    step__PharmacyStep["保険薬局での調剤"]:::step
    subgraph lane__PrescriptionFlow__Clinician["医師"]
      %% lane lane__PrescriptionFlow__Clinician: actor__Clinician
      step__InhouseStep["院内処方（引換券）"]:::step
      step__OrderStep["処方オーダ"]:::step
      step__OutsideStep["院外処方箋（電子処方箋管理サービス）"]:::step
    end
    subgraph lane__PrescriptionFlow__Pharmacist["薬剤師"]
      %% lane lane__PrescriptionFlow__Pharmacist: actor__Pharmacist
      step__CounselStep["服薬指導・薬剤交付"]:::step
      step__DispenseStep["調剤・監査"]:::step
    end
  end
  actor__Clinician(["医師"]):::actor
  %% desc actor__Clinician: 診察し、診断・オーダ・処方を出し、診療記録に署名する。
  actor__Pharmacist(["薬剤師"]):::actor
  %% desc actor__Pharmacist: 院内処方を調剤・監査し、服薬指導と交付を行う。疑わしい処方は医師に照会する。
  usecase__AnswerPharmacyInquiry("保険薬局からの疑義照会への回答"):::usecase
  %% desc usecase__AnswerPharmacyInquiry: 保険薬局からの疑義照会への回答。
  usecase__AnswerPrescriptionInquiry("疑義照会への回答"):::usecase
  %% desc usecase__AnswerPrescriptionInquiry: 医師の回答で引換券へ戻す。
  usecase__ConfirmDispense("保険薬局での調剤結果の受信"):::usecase
  %% desc usecase__ConfirmDispense: 保険薬局での調剤結果の受信。
  usecase__CounselMedication("服薬指導"):::usecase
  %% desc usecase__CounselMedication: 服薬指導。
  usecase__DispenseAndAudit("調剤・監査"):::usecase
  %% desc usecase__DispenseAndAudit: 薬剤師による調剤・監査。
  usecase__DraftPrescription("処方オーダの登録"):::usecase
  %% desc usecase__DraftPrescription: 処方オーダの作成。
  usecase__HandOverMedication("薬剤交付"):::usecase
  %% desc usecase__HandOverMedication: 薬剤交付。
  usecase__IssueInhouseVoucher("院内処方用引換券の交付"):::usecase
  %% desc usecase__IssueInhouseVoucher: 院内処方用引換券の交付。
  usecase__IssueOutsidePrescription("院外処方箋の発行"):::usecase
  %% desc usecase__IssueOutsidePrescription: 院外処方箋の発行と電子処方箋管理サービスへの登録。
  usecase__IssueRefillPrescription("リフィル処方箋の処方オーダ"):::usecase
  %% desc usecase__IssueRefillPrescription: リフィル処方箋の処方オーダ（使用回数の上限を記載）。
  usecase__RaisePrescriptionInquiry("疑義照会"):::usecase
  %% desc usecase__RaisePrescriptionInquiry: 薬剤師による疑義照会。
  actor__Clinician -.-> step__InquiryStep
  actor__Pharmacist -.-> step__InquiryStep
  step__CounselStep -.-> usecase__CounselMedication
  step__CounselStep -.-> usecase__HandOverMedication
  step__DispenseStep -.-> usecase__DispenseAndAudit
  step__DispenseStep -.->|excepts| step__InquiryStep
  step__DispenseStep -->|precedes| step__CounselStep
  step__InhouseStep -.-> usecase__IssueInhouseVoucher
  step__InhouseStep -->|precedes| step__DispenseStep
  step__InquiryStep -.-> usecase__AnswerPrescriptionInquiry
  step__InquiryStep -.-> usecase__RaisePrescriptionInquiry
  step__InquiryStep -->|repeats| step__DispenseStep
  step__OrderStep -->|branches| step__InhouseStep
  step__OrderStep -.-> usecase__DraftPrescription
  step__OrderStep -.-> usecase__IssueRefillPrescription
  step__OrderStep -->|precedes| step__OutsideStep
  step__OutsideStep -.-> usecase__IssueOutsidePrescription
  step__OutsideStep -->|precedes| step__PharmacyStep
  step__PharmacyStep -.-> usecase__AnswerPharmacyInquiry
  step__PharmacyStep -.-> usecase__ConfirmDispense
  classDef flow fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef step fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef lane fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef actor fill:#ffe3e3,stroke:#c92a2a,color:#000;
  classDef usecase fill:#d0ebff,stroke:#1864ab,color:#000;
```

### 計算・会計

```mermaid
%% rdra-ish diagram=flow scope=buc:BucVisitPayment model=samples/clinic-ops
%% kinds: flow step lane usecase event
%% omitted: buc
flowchart TD
  subgraph flow__VisitPaymentFlow["計算・会計の流れ"]
    subgraph lane__VisitPaymentFlow__AccountsClerk["医事課 会計担当"]
      %% lane lane__VisitPaymentFlow__AccountsClerk: actor__AccountsClerk
      step__CalculationStep["計算受付・会計計算入力"]:::step
      step__CallStep["会計の呼出"]:::step
      step__ReceiptStep["領収証・明細書の交付"]:::step
      step__RefundStep["還付・追加徴収"]:::step
      step__WindowStep["会計窓口で精算"]:::step
    end
    subgraph lane__VisitPaymentFlow__Patient["患者"]
      %% lane lane__VisitPaymentFlow__Patient: actor__Patient
      step__KioskStep["自動精算機で精算"]:::step
    end
  end
  usecase__ApplySelectedTreatmentFee("選定療養費の算定"):::usecase
  %% desc usecase__ApplySelectedTreatmentFee: 紹介状なし初診だけに選定療養費の Charge を作る。
  usecase__CalculateVisitBill("会計計算入力"):::usecase
  %% desc usecase__CalculateVisitBill: 会計計算入力で予約を会計待ちにする。
  usecase__CallPatientForPayment("会計の呼出"):::usecase
  %% desc usecase__CallPatientForPayment: 会計の呼出（参照のみ）。
  usecase__IssueReceipt("領収証・明細書の交付"):::usecase
  %% desc usecase__IssueReceipt: 領収証・明細書の交付で予約を完了にする。
  usecase__RefundPayment("還付・追加徴収"):::usecase
  %% desc usecase__RefundPayment: 保険の変更や計算誤りによる還付・追加徴収で精算を還付済にする。
  usecase__SettleAtKiosk("自動精算機での精算"):::usecase
  %% desc usecase__SettleAtKiosk: 自動精算機での精算。
  usecase__SettleAtWindow("会計窓口での精算"):::usecase
  %% desc usecase__SettleAtWindow: 会計窓口での精算。
  event__EvPaymentSettled>"自動精算機での精算完了"]:::event
  step__CalculationStep -.-> usecase__ApplySelectedTreatmentFee
  step__CalculationStep -.-> usecase__CalculateVisitBill
  step__CalculationStep -->|precedes| step__CallStep
  step__CallStep -->|branches| step__KioskStep
  step__CallStep -.-> usecase__CallPatientForPayment
  step__CallStep -->|precedes| step__WindowStep
  step__KioskStep -.-> event__EvPaymentSettled
  step__KioskStep -.-> usecase__SettleAtKiosk
  step__KioskStep -->|precedes| step__ReceiptStep
  step__ReceiptStep -.-> usecase__IssueReceipt
  step__ReceiptStep -.->|excepts| step__RefundStep
  step__RefundStep -.-> usecase__RefundPayment
  step__WindowStep -.-> usecase__SettleAtWindow
  step__WindowStep -->|precedes| step__ReceiptStep
  classDef flow fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef step fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef lane fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef usecase fill:#d0ebff,stroke:#1864ab,color:#000;
  classDef event fill:#ffdeeb,stroke:#a61e4d,color:#000;
```

### レセプト作成・請求

```mermaid
%% rdra-ish diagram=flow scope=buc:BucBillingClaims model=samples/clinic-ops
%% kinds: flow step lane timer usecase
%% omitted: buc
flowchart TD
  subgraph flow__ClaimCycleFlow["レセプトの月次サイクル"]
    subgraph lane__ClaimCycleFlow__ClaimsClerk["医事課 レセプト担当"]
      %% lane lane__ClaimCycleFlow__ClaimsClerk: actor__ClaimsClerk
      step__AssessStep["査定と再審査請求"]:::step
      step__CreateStatementStep["レセプト作成（月初）"]:::step
      step__HoldStep["保留（月遅れ）"]:::step
      step__InspectStep["院内点検"]:::step
      step__PaidStep["支払（診療翌々月）"]:::step
      step__ReceiptErrorStep["受付不能・要確認の再送（11日・12日）"]:::step
      step__ReturnStep["返戻と再請求"]:::step
      step__ReviewStep["審査"]:::step
      step__SubmitStep["請求（5日～10日）"]:::step
    end
  end
  timer__ErrorResendWindow(("エラー分レセプトの再送（毎月11日・12日）")):::timer
  timer__MonthlyClaimCreation(("前月分レセプトの作成（月初）")):::timer
  timer__MonthlyClaimDeadline(("前月分レセプトの請求（毎月5日～10日）")):::timer
  usecase__CorrectReturnedStatement("返戻レセプトの訂正（再請求）"):::usecase
  %% desc usecase__CorrectReturnedStatement: 返戻の訂正と再請求。
  usecase__GenerateClaimStatement("レセプト作成"):::usecase
  %% desc usecase__GenerateClaimStatement: 月初のレセプト作成。
  usecase__HoldClaimStatement("レセプトの保留（月遅れ）"):::usecase
  %% desc usecase__HoldClaimStatement: 保留（月遅れ）。
  usecase__InspectClaimStatement("レセプトの院内点検"):::usecase
  %% desc usecase__InspectClaimStatement: 院内点検。
  usecase__PostPayment("診療報酬の入金"):::usecase
  %% desc usecase__PostPayment: 診療翌々月の入金。
  usecase__ReceiveClaimAccepted("レセプト受付（審査へ）"):::usecase
  %% desc usecase__ReceiveClaimAccepted: 審査支払機関の受付で審査中になる。
  usecase__RecordClaimAssessment("査定の取込"):::usecase
  %% desc usecase__RecordClaimAssessment: 査定の取込。
  usecase__RecordClaimReturn("返戻レセプトの取込"):::usecase
  %% desc usecase__RecordClaimReturn: 返戻レセプトの取込（3か月以内）。
  usecase__RecordReceiptError("受付不能・要確認の確認"):::usecase
  %% desc usecase__RecordReceiptError: 受付不能・要確認の確認。
  usecase__ReleaseHeldStatement("保留レセプトの再提出準備"):::usecase
  %% desc usecase__ReleaseHeldStatement: 保留レセプトの再提出準備。
  usecase__RequestReexamination("再審査請求"):::usecase
  %% desc usecase__RequestReexamination: 再審査請求。
  usecase__ResendErroredStatement("エラー分レセプトの再送"):::usecase
  %% desc usecase__ResendErroredStatement: 11日・12日のエラー分再送。
  usecase__ReviewClaimTransfer("資格の振替・分割の確認"):::usecase
  %% desc usecase__ReviewClaimTransfer: 資格の振替・分割の確認（参照のみ）。
  usecase__SubmitClaimStatements("レセプト請求"):::usecase
  %% desc usecase__SubmitClaimStatements: 毎月10日までの請求。
  step__AssessStep -.-> usecase__RecordClaimAssessment
  step__AssessStep -.-> usecase__RequestReexamination
  step__AssessStep -->|precedes| step__PaidStep
  step__CreateStatementStep -.-> timer__MonthlyClaimCreation
  step__CreateStatementStep -.-> usecase__GenerateClaimStatement
  step__CreateStatementStep -->|precedes| step__InspectStep
  step__HoldStep -.-> usecase__HoldClaimStatement
  step__HoldStep -.-> usecase__ReleaseHeldStatement
  step__HoldStep -->|repeats| step__InspectStep
  step__InspectStep -->|branches| step__HoldStep
  step__InspectStep -.-> usecase__InspectClaimStatement
  step__InspectStep -->|precedes| step__SubmitStep
  step__PaidStep -.-> usecase__PostPayment
  step__ReceiptErrorStep -.-> timer__ErrorResendWindow
  step__ReceiptErrorStep -.-> usecase__RecordReceiptError
  step__ReceiptErrorStep -.-> usecase__ResendErroredStatement
  step__ReceiptErrorStep -->|repeats| step__SubmitStep
  step__ReturnStep -.-> usecase__CorrectReturnedStatement
  step__ReturnStep -.-> usecase__RecordClaimReturn
  step__ReturnStep -->|repeats| step__InspectStep
  step__ReviewStep -->|branches| step__AssessStep
  step__ReviewStep -.-> usecase__ReceiveClaimAccepted
  step__ReviewStep -.-> usecase__ReviewClaimTransfer
  step__ReviewStep -.->|excepts| step__ReturnStep
  step__ReviewStep -->|precedes| step__PaidStep
  step__SubmitStep -.-> timer__MonthlyClaimDeadline
  step__SubmitStep -.-> usecase__SubmitClaimStatements
  step__SubmitStep -.->|excepts| step__ReceiptErrorStep
  step__SubmitStep -->|precedes| step__ReviewStep
  classDef flow fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef step fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef lane fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef timer fill:#ffe8cc,stroke:#d9480f,color:#000;
  classDef usecase fill:#d0ebff,stroke:#1864ab,color:#000;
```

### フォローアップ

```mermaid
%% rdra-ish diagram=flow scope=buc:BucFollowupCare model=samples/clinic-ops
%% kinds: flow step lane usecase
%% omitted: buc
flowchart TD
  subgraph flow__FollowupFlow["フォローアップの流れ"]
    subgraph lane__FollowupFlow__CareCoordinator["患者支援・地域連携担当"]
      %% lane lane__FollowupFlow__CareCoordinator: actor__CareCoordinator
      step__CarePlanStep["ケアプランの作成"]:::step
      step__FollowUpMsgStep["患者への連絡"]:::step
      step__PlanCloseStep["ケアプランの終了"]:::step
      step__ResponseStep["患者応答の確認"]:::step
      step__ResultNoticeStep["検査結果の患者通知"]:::step
      step__RevisitStep["再診の予約"]:::step
    end
  end
  usecase__CloseCarePlan("ケアプランの終了"):::usecase
  %% desc usecase__CloseCarePlan: ケアプラン終了。
  usecase__CreateCarePlan("ケアプランの作成"):::usecase
  %% desc usecase__CreateCarePlan: 署名イベントからのケアプラン作成。
  usecase__NotifyPatientResult("検査結果の患者通知"):::usecase
  %% desc usecase__NotifyPatientResult: 結果の患者通知。
  usecase__ReviewPatientResponse("患者応答の確認"):::usecase
  %% desc usecase__ReviewPatientResponse: 患者応答の確認。
  usecase__ScheduleFollowUpVisit("再診の予約"):::usecase
  %% desc usecase__ScheduleFollowUpVisit: 再診予約。
  usecase__SendFollowUpMessage("患者への連絡"):::usecase
  %% desc usecase__SendFollowUpMessage: 患者連絡。
  step__CarePlanStep -->|branches| step__ResultNoticeStep
  step__CarePlanStep -.-> usecase__CreateCarePlan
  step__CarePlanStep -->|precedes| step__FollowUpMsgStep
  step__FollowUpMsgStep -.-> usecase__SendFollowUpMessage
  step__FollowUpMsgStep -->|precedes| step__ResponseStep
  step__PlanCloseStep -.-> usecase__CloseCarePlan
  step__ResponseStep -.-> usecase__ReviewPatientResponse
  step__ResponseStep -->|precedes| step__RevisitStep
  step__ResultNoticeStep -.-> usecase__NotifyPatientResult
  step__ResultNoticeStep -->|precedes| step__ResponseStep
  step__RevisitStep -.-> usecase__ScheduleFollowUpVisit
  step__RevisitStep -->|precedes| step__PlanCloseStep
  classDef flow fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef step fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef lane fill:#d3f9d8,stroke:#2b8a3e,color:#000;
  classDef usecase fill:#d0ebff,stroke:#1864ab,color:#000;
```

## 6. ビジネスルール（受け入れ条件）

`list --kind rule --format markdown` の出力である。各行が、システムが守る業務規則で、受け入れ試験の条件になる。

| kind | label | entities | rule | applies_to | site |
| --- | --- | --- | --- | --- | --- |
| condition | 振替・分割の対象外（負担割合の相違・公費） |  |  | RecordClaimReturn | samples/clinic-ops/buc/buc_billing_claims.rdra:178 |
| condition | 医師が確定したオーダ | ClinicalOrder | ClinicalOrder.physician_confirmed == true, ClinicalOrder.status == オーダ済 | CollectSpecimen | samples/clinic-ops/buc/buc_orders_results.rdra:111 |
| condition | 予約済みの患者 | Appointment | (Appointment.status == 予約済).along(InsurancePolicy, PatientAccount, Appointment) | BatchVerifyEligibility | samples/clinic-ops/buc/buc_visit_checkin.rdra:106 |
| condition | 窓口負担は負担割合で計算し、限度額情報があれば限度額までとする |  |  | CalculateVisitBill | samples/clinic-ops/buc/buc_visit_payment.rdra:73 |
| condition | 紹介状なしの初診で、除外に当たらない | Charge\|Appointment | Charge.kind == 選定療養（自費）, Appointment.referral == 紹介状なし初診 | ApplySelectedTreatmentFee | samples/clinic-ops/buc/buc_visit_payment.rdra:92 |
| invariant | 受付済の予約には受付時刻が記録されている | Appointment | when Appointment.status == 受付済 then Appointment.checked_in_at == present |  | samples/clinic-ops/shared/entities.rdra:430 |
| invariant | 完了した予約には完了時刻が記録されている | Appointment | when Appointment.status == 完了 then Appointment.completed_at == present |  | samples/clinic-ops/shared/entities.rdra:432 |
| invariant | 署名済の診療記録には署名時刻が記録されている | Encounter | when Encounter.status == 署名済 then Encounter.signed_at == present |  | samples/clinic-ops/shared/entities.rdra:434 |
| invariant | 結果が出たオーダには結果日時が記録されている | ClinicalOrder | when ClinicalOrder.status == 結果受信 then ClinicalOrder.resulted_at == present |  | samples/clinic-ops/shared/entities.rdra:436 |
| invariant | 発行済の処方箋には送信日時が記録されている | Prescription | when Prescription.status == 院外処方箋発行済 then Prescription.sent_at == present |  | samples/clinic-ops/shared/entities.rdra:438 |
| invariant | 支払済のレセプトには支払日時が記録されている | ClaimStatement | when ClaimStatement.status == 支払済 then ClaimStatement.paid_at == present |  | samples/clinic-ops/shared/entities.rdra:440 |
| forbidden | 訂正した返戻は、再請求の前に返戻理由を消す | ClaimStatement | ClaimStatement.status == 支払済 AND ClaimStatement.return_reason == present |  | samples/clinic-ops/shared/entities.rdra:442 |
| forbidden | 医師事務作業補助者が入力したオーダは、医師が確定してから検体を採取する | ClinicalOrder | ClinicalOrder.status == 採取済 AND ClinicalOrder.physician_confirmed == false |  | samples/clinic-ops/shared/entities.rdra:445 |

## 7. 非機能要件

| id | label | metric | target | window | slo | availability | resilience | audit | logging | retention | privacy | description |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| ScheduleLookupLatency | 予約枠検索の応答時間 | p95_latency_ms | <=500 | 5m | 99.5% | multi_az | retryable |  |  |  |  |  |

| id | label | metric | target | window | slo | availability | resilience | audit | logging | retention | privacy | description |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| BillingAuditTrail | 会計の監査証跡 |  |  |  |  |  |  | enabled |  | 7y |  |  |
| NoPharmacySteering | 特定の保険薬局において調剤を受けるべき旨の指示等を行わない |  |  |  |  |  |  | enabled |  |  |  |  |
| PatientDataPrivacy | 患者情報の保護 |  |  |  |  |  |  |  | structured | 10y | restricted |  |
