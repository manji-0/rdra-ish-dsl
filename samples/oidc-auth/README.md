# oidc-auth

oidc-auth models an OpenID Provider (OP) and every OpenID Connect authentication scenario its
clients use: discovery and dynamic registration, the authorization code, implicit, and hybrid
flows with every response type, PKCE, the `prompt`, `max_age`, and `acr_values` parameters and
the errors a request that may show no page answers, consent, code redemption and UserInfo,
refresh-token rotation, revocation and introspection, the device authorization grant, CIBA in
poll, ping, and push mode, RP-initiated, front-channel, and back-channel logout, session
management, third-party initiated login, and signing-key rotation.

The labels and comments are in English. The `//` comments cite the specification each rule
comes from: "Core §x" is OpenID Connect Core 1.0, and the other specifications are named in
full (RFC 6749, RFC 7636, RFC 7009, RFC 7662, RFC 8628, RFC 9700, OpenID Connect Discovery 1.0,
Dynamic Client Registration 1.0, CIBA Core 1.0, RP-Initiated Logout 1.0, Front-Channel Logout
1.0, Back-Channel Logout 1.0, and Session Management 1.0). Every status name, the lifetimes,
and the goals' numbers are the model's assumptions.

```sh
rdra-ish check samples/oidc-auth
rdra-ish next samples/oidc-auth
rdra-ish list samples/oidc-auth --kind rule --format markdown
rdra-ish diagram samples/oidc-auth --kind flow --buc BucBrowserSignIn
```

## Scenarios

| Scenario | Specification | BUC | Use cases |
|---|---|---|---|
| Discovery and the JWK Set | Discovery 1.0 §4; Core §10.1.1 | `BucClientRegistration` | `DiscoverProvider`, `FetchJwks` |
| Dynamic client registration | Dynamic Client Registration 1.0 §3, §4 | `BucClientRegistration` | `RegisterClient`, `ReadClientRegistration` |
| Client suspension | (OP policy) | `BucClientRegistration` | `SuspendClient`, `ReinstateClient` |
| Signing-key rotation | Core §10.1.1 | `BucSigningKeys` | `PublishNextKey`, `ActivateKey`, `RetireKey` |
| Authorization Code Flow (`code`) | Core §3.1 | `BucBrowserSignIn`, `BucCodeExchange` | `IssueCodeResponse`, `RedeemCode` |
| PKCE, and its absence for a public client | RFC 7636; RFC 9700 §2.1.1 | `BucBrowserSignIn` | `ValidateRequest`, `RejectInvalidRequest` |
| Implicit Flow (`id_token`, `id_token token`) | Core §3.2 | `BucBrowserSignIn` | `IssueIdTokenResponse`, `IssueImplicitTokenResponse` |
| Hybrid Flow (`code id_token`, `code token`, `code id_token token`) | Core §3.3 | `BucBrowserSignIn`, `BucCodeExchange` | `IssueHybridIdTokenResponse`, `IssueHybridTokenResponse`, `IssueHybridFullResponse`, `RedeemCode` |
| Login, single sign-on, `prompt=login`, `prompt=select_account` | Core §3.1.2.1, §3.1.2.3 | `BucBrowserSignIn` | `LogIn`, `AuthenticateEndUser`, `ReuseSession`, `SelectAccount` |
| Step-up with `acr_values` | Core §3.1.2.1, §2 | `BucBrowserSignIn` | `StepUpAuthentication` |
| Consent, its reuse, administrative consent, denial | Core §3.1.2.4; RFC 6749 §4.1.2.1 | `BucBrowserSignIn` | `GrantConsent`, `ReuseConsent`, `RecordAdministrativeConsent`, `DenyConsent` |
| `prompt=none` errors | Core §3.1.2.6 | `BucBrowserSignIn` | `ReturnLoginRequired`, `ReturnAccountSelectionRequired`, `ReturnInteractionRequired`, `ReturnConsentRequired` |
| Abandoned login | (OP policy) | `BucBrowserSignIn` | `AbandonRequest` |
| Third-party initiated login | Core §4 | `BucBrowserSignIn` | `InitiateThirdPartyLogin` |
| Code replay and code expiry | RFC 6749 §4.1.2 | `BucCodeExchange` | `RejectCodeReplay`, `ExpireCode` |
| UserInfo | Core §5.3 | `BucCodeExchange` | `GetUserInfo` |
| Refresh with rotation, and reuse detection | Core §12; RFC 9700 §4.14.2 | `BucTokenMaintenance` | `RefreshTokens`, `DetectRefreshTokenReuse` |
| Revocation and introspection | RFC 7009; RFC 7662 | `BucTokenMaintenance` | `RevokeRefreshToken`, `RevokeAccessToken`, `IntrospectToken`, `RevokeGrantAccessTokens`, `RevokeGrantRefreshTokens` |
| Consent withdrawal | Core §3.1.2.4 | `BucTokenMaintenance` | `WithdrawConsent`, `RevokeGrantOnWithdrawal` |
| Device authorization grant | RFC 8628 | `BucDeviceSignIn` | `StartDeviceAuthorization`, `ApproveDevice`, `DenyDevice`, `PollDevicePending`, `RedeemDeviceCode`, `ExpireDeviceCode` |
| CIBA in poll, ping, and push mode | CIBA Core 1.0 §5, §7, §10 | `BucBackchannelSignIn` | `StartBackchannelAuthentication`, `RecordDeviceApproval`, `RecordDeviceDenial`, `PollBackchannelToken`, `SendPingCallback`, `PushBackchannelTokens`, `ExpireBackchannelRequest` |
| RP-initiated logout, logout at the OP, session expiry | RP-Initiated Logout 1.0 | `BucLogout` | `LogOutFromRp`, `LogOutAtOp`, `ExpireSession` |
| Back-channel and front-channel logout | Back-Channel Logout 1.0; Front-Channel Logout 1.0 | `BucLogout` | `SendBackchannelLogout`, `RecordLogoutDelivered`, `RecordLogoutFailed`, `RenderFrontchannelLogout` |
| Session management | Session Management 1.0 | `BucLogout` | `CheckSessionState` |

Out of scope, each an `out-of-scope` requirement: the client credentials grant (no End-User is
authenticated), the resource owner password credentials grant (RFC 9700 §2.4 forbids it),
self-issued OPs, and the encodings of a request (request objects, `request_uri`, PAR, the
`claims` parameter), which reach the same validation and flow as plain parameters.

## Modeling choices

- **The RP is an actor; its callback endpoints are an external system.** The RP calls the
  OP's endpoints, so it performs use cases and holds permissions (`ClientCredential`,
  `BearerAccessToken`, `RegistrationAccess`). The endpoints the OP calls (the backchannel
  logout URI, the CIBA notification endpoint, the `initiate_login_uri`) are
  `RpCallbackEndpoint`, which talks to the model through events. The CIBA authentication
  device is an external system too.
- **The RP's back-channel calls follow what the OP delivered.** `RedeemCode` runs on
  `EvCodeIssued`; UserInfo, refresh, revocation, and introspection run on `EvTokensIssued` or
  `EvFrontChannelAccessToken`. The RP's own schedules are timers (`RpMetadataRefresh`,
  `SessionCheckInterval`).
- **One use case per response type.** `EvRequestAuthorized` starts all six, and a condition on
  `response_type` lets exactly one run. Each creates what Core §3 says comes back on the
  redirect URI: a code, an ID Token, an access token, or a combination.
- **The interaction is apart from the request** (`AdrInteractionApart`). `prompt`, `acr`, and
  how the End-User was authenticated are on `Interaction`, joined 1:1 to the request and
  deleted with the response. The request keeps the protocol parameters (`response_type`,
  `pkce`, `nonce`). On one row the browser flow passes the 256 patterns `check` allows an
  entity.
- **The OP session is started apart from any request** (`AdrLoginApartFromRequest`).
  `LogIn` creates the session on its own, as the OP's login page and account page both do;
  `AuthenticateEndUser` (a login made for the request) and `ReuseSession` (single sign-on)
  link the interaction to it. So the logout's cone holds the session, the client, and the
  notice, and not the request that led to the login.
- **A validation step and an `OR` as arms.** `ValidateRequest` moves the request to
  `validated` on one of three arms: no code is issued, the client is confidential, or the
  challenge is S256. `RejectInvalidRequest` takes the rest. The session serves a request on
  one of two arms: the session is multi-factor, or the request asks for no second factor.
- **One row per token family.** `RefreshToken` is the rotation family: a refresh moves it to
  `rotated`, and an old value coming back (`DetectRefreshTokenReuse`) revokes it and the
  grant. `AccessToken` is the token most recently issued under a grant; a refresh replaces
  it. A revoked grant reaches its tokens through `EvGrantRevoked`.
- **No token rows for the device and CIBA flows** (`AdrNoTokenRowsForDecoupledFlows`). The
  two flows end at `redeemed` or `pushed` on their own request. Creating grants and tokens
  there too would join three independent flows in every token cone, past the state cap.
- **Error codes are use cases.** Which error a response carries is the use case that sent it
  (`ReturnLoginRequired`, `DenyConsent`, ...), so the request keeps no error column.

## Rules

`shared/rules.rdra` holds 13 labelled rules. Each fails `check` when what protects it is
removed. Where a row names several guards, removing any one of them fails the rule. The
Logout Token rule needs both its guard and its trigger removed, since either alone keeps it.
`rdra-ish check samples/oidc-auth --guards` drops each guard in turn and lists the rules
that then fail; `crates/rdra-ish-cli/tests/guards_cli.rs` reads this table and checks each
row against that report. A rule protected by effects, a `links`, or a guard and a trigger
together depends on no single guard, so the report lists no guard for it. Removing the
`.when` arms or values of a use case leaves every arm or value of that column open.

| Rule | Protected by |
|---|---|
| A public client gets no code without an S256 code challenge | the `.when` arms of `ValidateRequest` |
| An ID Token from the authorization endpoint always answers a nonce | the `.when` values of `sets(RequestAuthentication, AuthorizationRequest.nonce == ..)` |
| prompt=none never shows the login page | `RequestValidated` on `AuthenticateEndUser` |
| prompt=login never reuses the session without a login | `SessionMayServe` on `ReuseSession`, `AccountChoiceAsked` on `SelectAccount` |
| A request is authorized only after the End-User is authenticated | `EndUserAuthenticated` on `GrantConsent` |
| A request asking for a second factor is served only by a multi-factor session | the `.when` arms of `AuthenticateEndUser`, `ReuseSession`, and `SelectAccount` |
| The Implicit Flow issues no refresh token | the token sets of the implicit responses |
| A replayed code leaves its grant revoked | the `Grant.status` transitions of `RejectCodeReplay` |
| An approved device authorization names the End-User who approved it | `links(ApproveDevice, DeviceAuthorization, UserAccount)` |
| A poll-mode client is neither pinged nor pushed to | `PingMode` on `SendPingCallback`, `PushMode` on `PushBackchannelTokens` |
| A ping-mode client is not pushed the tokens | `PushMode` on `PushBackchannelTokens` |
| A push-mode client is neither pinged nor polls | `PingMode` on `SendPingCallback`, the `.when` arms of `PollBackchannelToken` |
| A Logout Token is sent only for a session that ended | `SessionEnded` on `SendBackchannelLogout` and `triggers(EvSessionEnded, SendBackchannelLogout)` together |

## Known gaps

- **A recorded consent is found through the client, not the End-User.** `ConsentOnRecord`
  and `SilentWithoutConsent` follow `Interaction → AuthorizationRequest → Client → Consent`.
  A key path cannot also require the consent's End-User to be the session's, so at two
  instances one End-User's consent could serve another's request.
- **A suspended client is checked only in the device and CIBA flows.** Reading
  `Client.status` in the browser flow doubles its states, which passes the state cap once
  the token lifecycle joins it. The preconditions of `RequestAuthentication` and `RedeemCode`
  state the check in words.
- **The cookie is not modeled.** With `prompt=none` and no session linked yet, the OP may
  answer `login_required` or `account_selection_required`: the model does not know how
  many sessions the user agent holds. `max_age` is review text on `ReuseSession`; the model
  has no clock comparison for `auth_time`.
- **UserInfo, introspection, the session check, and the front-channel iframes write
  nothing**, so no guard can be put on them. That a revoked token is refused there is review
  text; the token cascade (`RevokeGrantAccessTokens`, `RevokeGrantRefreshTokens`) runs after
  the grant is revoked, so a rule that a revoked grant has no active token would fail
  between the two steps.
- **One Logout Token per ended session.** `SendBackchannelLogout` binds one client, not each
  client the session signed in to; the model has no key from a session to the clients it
  served.
- **At one instance a hybrid grant is never refreshed.** The hybrid responses already use the
  one `IdToken` or `AccessToken` slot that `RedeemCode` would create, so
  `out/states_grant.txt` shows refreshed grants of the code flow only. At two instances the
  redemption has its own rows.
- **`verify` runs at one instance per entity.** `verify --prove --instance-count 1` decides
  every check (9 cones, the largest 30,277 states, about 80 s) and proves all 16 rules with z3 for
  any number of rows. At the default two instances, the cones of the browser flow exceed
  two million states and do not finish in 15 GB of memory. Only the decoupled entities
  pass at two instances:

  ```sh
  rdra-ish verify samples/oidc-auth --instance-count 1 --prove
  rdra-ish verify samples/oidc-auth --entity BackchannelRequest --entity DeviceAuthorization \
    --entity LogoutNotice --entity SigningKey
  ```

## Diagnostics left in place

`check` reports nothing. `lint` reports `api-contract-missing` for the 20 internal steps that
are APIs only so that each writes its own rows (`RequestValidationApi`, `ErrorResponseApi`,
the expiry and cascade steps, and so on); they are not endpoints an RP calls. It reports
`screen-no-transition` for the four screens that are the only screen of their BUC.
