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
    pub struct ConceptKey;
    pub struct DomainObjectKey;
    pub struct AggregateKey;
    pub struct ValueObjectKey;
    pub struct BusinessKey;
    pub struct BucKey;
    pub struct FlowKey;
    pub struct StepKey;
    pub struct UsageSceneKey;
    pub struct UseCaseKey;
    pub struct ScreenKey;
    pub struct FieldKey;
    pub struct EventKey;
    pub struct EntityKey;
    pub struct StateKey;
    pub struct ConditionKey;
    pub struct VariationKey;
    pub struct ApiKey;
    pub struct DtoKey;
    pub struct LocationKey;
    pub struct TimingKey;
    pub struct MediumKey;
    pub struct PermissionKey;
}
