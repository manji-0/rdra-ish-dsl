use slotmap::new_key_type;

// --- Key types ---
new_key_type! {
    pub struct ActorKey;
    pub struct ExtSystemKey;
    pub struct SystemKey;
    pub struct RequirementKey;
    pub struct AdrKey;
    pub struct NfrKey;
    pub struct QualityKey;
    pub struct ConstraintKey;
    pub struct BusinessKey;
    pub struct BucKey;
    pub struct FlowKey;
    pub struct StepKey;
    pub struct UseCaseKey;
    pub struct ScreenKey;
    pub struct FieldKey;
    pub struct EventKey;
    pub struct EntityKey;
    pub struct StateKey;
    pub struct ApiKey;
    pub struct DtoKey;
    pub struct TimerKey;
    pub struct MediumKey;
    pub struct PermissionKey;
    pub struct VariationKey;
}
