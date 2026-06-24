use super::comparison::ComparisonProp;
use super::keys::*;
use super::refs::{
    AppliesToTarget, Cardinality, ConceptualRef, ConstrainsTarget, ContainedRef, ContainerRef,
    CoversTarget, DataOrigin, DecidesTarget, EntityTouchpoint, NfrOrConstraint, PerformTarget,
    TriggerTarget,
};

/// 解析済み述語の型付き表現（discriminated union）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypedPredicate {
    Performs {
        actor: ActorKey,
        target: PerformTarget,
    },
    Uses {
        actor: ActorKey,
        ext_system: ExtSystemKey,
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
        nfr: NfrKey,
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
        usecase: UseCaseKey,
        event: EventKey,
    },
    Triggers {
        event: EventKey,
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
    MapsTo {
        source: ConceptualRef,
        entity: EntityKey,
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
        event: EventKey,
        from: StateKey,
        to: StateKey,
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
    },
    SetsColumn {
        origin: DataOrigin,
        entity: EntityKey,
        column: std::string::String,
    },
    SetsProposition {
        origin: DataOrigin,
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
