use super::comparison::ComparisonProp;
use super::keys::*;
use super::refs::{
    AppliesSource, AppliesToTarget, Cardinality, CombinesTarget, ConstrainsTarget, ContainedRef,
    ContainerRef, CoversTarget, DecidesTarget, EntityTouchpoint, NfrOrConstraint, PerformTarget,
    Raiser, TriggerSource, TriggerTarget,
};

/// 解析済み述語の型付き表現（discriminated union）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypedPredicate {
    Performs {
        actor: ActorKey,
        target: PerformTarget,
    },
    Reads {
        origin: EntityTouchpoint,
        entity: EntityKey,
    },
    Writes {
        origin: EntityTouchpoint,
        entity: EntityKey,
    },
    Creates {
        origin: EntityTouchpoint,
        entity: EntityKey,
    },
    Updates {
        origin: EntityTouchpoint,
        entity: EntityKey,
    },
    Deletes {
        origin: EntityTouchpoint,
        entity: EntityKey,
    },
    Invokes {
        usecase: UseCaseKey,
        api: ApiKey,
    },
    Request {
        api: ApiKey,
        dto: DtoKey,
    },
    Response {
        api: ApiKey,
        dto: DtoKey,
    },
    ErrorResponse {
        api: ApiKey,
        dto: DtoKey,
    },
    AppliesTo {
        source: AppliesSource,
        target: AppliesToTarget,
    },
    Qualifies {
        source: NfrOrConstraint,
        quality: QualityKey,
    },
    Constrains {
        constraint: ConstraintKey,
        target: ConstrainsTarget,
    },
    Owns {
        system: SystemKey,
        entity: EntityKey,
    },
    Displays {
        usecase: UseCaseKey,
        screen: ScreenKey,
    },
    Shows {
        screen: ScreenKey,
        entity: EntityKey,
    },
    Raises {
        raiser: Raiser,
        event: EventKey,
    },
    Triggers {
        source: TriggerSource,
        target: TriggerTarget,
    },
    Contains {
        container: ContainerRef,
        contained: ContainedRef,
    },
    Precedes {
        from: StepKey,
        to: StepKey,
    },
    Branches {
        from: StepKey,
        to: StepKey,
    },
    Excepts {
        from: StepKey,
        to: StepKey,
    },
    Repeats {
        from: StepKey,
        to: StepKey,
    },
    Covers {
        step: StepKey,
        target: CoversTarget,
    },
    Compensates {
        from: UseCaseKey,
        to: UseCaseKey,
    },
    Coordinates {
        usecase: UseCaseKey,
        left: EntityKey,
        right: EntityKey,
    },
    Belongs {
        buc: BucKey,
        business: BusinessKey,
    },
    HasPermission {
        actor: ActorKey,
        permission: PermissionKey,
    },
    RequiresPermission {
        origin: EntityTouchpoint,
        permission: PermissionKey,
    },
    RequiresMedium {
        origin: EntityTouchpoint,
        medium: MediumKey,
    },
    Motivates {
        requirement: RequirementKey,
        buc: BucKey,
    },
    Decides {
        adr: AdrKey,
        target: DecidesTarget,
    },
    Transitions {
        entity: EntityKey,
        column: std::string::String,
        usecase: UseCaseKey,
        from: std::string::String,
        to: std::string::String,
    },
    Combines {
        condition: ConditionKey,
        target: CombinesTarget,
    },
    Holds {
        condition: ConditionKey,
    },
    Outbox {
        event: EventKey,
    },
    MapsField {
        field: FieldKey,
        entity: EntityKey,
        column: std::string::String,
    },
    Relate {
        from: EntityKey,
        to: EntityKey,
        cardinality: Cardinality,
        via: Option<EntityKey>,
    },
    SetsColumn {
        origin: UseCaseKey,
        entity: EntityKey,
        column: std::string::String,
    },
    SetsProposition {
        origin: UseCaseKey,
        entity: EntityKey,
        prop: ComparisonProp,
        truth: bool,
    },
    After {
        anchor: UseCaseKey,
    },
    Forbidden {
        entity: EntityKey,
    },
    Required {
        entity: EntityKey,
    },
    Exclusive {
        entity: EntityKey,
    },
    Invariant {
        entity: EntityKey,
    },
    ForbiddenWhen {
        entity: EntityKey,
    },
    CrossForbidden {
        scope: Vec<EntityKey>,
    },
    CrossInvariant {
        scope: Vec<EntityKey>,
    },
}
