use crate::{ValidationError, ValidationIssue};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Version of the text-backed model schema understood by this crate.
pub const SCHEMA_VERSION: u32 = 1;

fn default_schema_version() -> u32 {
    SCHEMA_VERSION
}

/// A complete text-backed system model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Model {
    /// Version of the TOML schema, independent of the crate version.
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    /// Human-readable model name.
    pub name: String,
    /// Definitions, usages, packages, and supporting elements.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub elements: Vec<Element>,
    /// Directed relationships between elements.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relationships: Vec<Relationship>,
}

impl Model {
    /// Create an empty model using the current text schema.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            name: name.into(),
            elements: Vec::new(),
            relationships: Vec::new(),
        }
    }

    /// Add an element to the model.
    pub fn add_element(&mut self, element: Element) -> &mut Self {
        self.elements.push(element);
        self
    }

    /// Add a relationship to the model.
    pub fn add_relationship(&mut self, relationship: Relationship) -> &mut Self {
        self.relationships.push(relationship);
        self
    }

    /// Find an element by its stable model identifier.
    pub fn element(&self, id: &str) -> Option<&Element> {
        self.elements.iter().find(|element| element.id == id)
    }

    /// Return basic counts suitable for CLI and tooling output.
    pub fn summary(&self) -> ModelSummary {
        ModelSummary {
            elements: self.elements.len(),
            definitions: self
                .elements
                .iter()
                .filter(|element| element.kind.role() == Some(ElementRole::Definition))
                .count(),
            usages: self
                .elements
                .iter()
                .filter(|element| element.kind.role() == Some(ElementRole::Usage))
                .count(),
            relationships: self.relationships.len(),
        }
    }

    /// Validate identifiers, references, multiplicities, and relationship roles.
    pub fn validate(&self) -> Result<(), ValidationError> {
        let mut issues = Vec::new();

        if self.schema_version != SCHEMA_VERSION {
            issues.push(ValidationIssue::new(
                "schema_version",
                format!(
                    "unsupported schema version {}; expected {SCHEMA_VERSION}",
                    self.schema_version
                ),
            ));
        }
        if self.name.trim().is_empty() {
            issues.push(ValidationIssue::new("name", "model name must not be empty"));
        }

        let mut elements = HashMap::new();
        let mut all_ids = HashSet::new();
        for (index, element) in self.elements.iter().enumerate() {
            let location = format!("elements[{index}]");
            if element.id.trim().is_empty() {
                issues.push(ValidationIssue::new(
                    format!("{location}.id"),
                    "element id must not be empty",
                ));
            } else if !all_ids.insert(element.id.as_str()) {
                issues.push(ValidationIssue::new(
                    format!("{location}.id"),
                    format!("duplicate model id {:?}", element.id),
                ));
            } else {
                elements.insert(element.id.as_str(), element);
            }
            if element.name.trim().is_empty() {
                issues.push(ValidationIssue::new(
                    format!("{location}.name"),
                    "element name must not be empty",
                ));
            }
            if let Some(multiplicity) = &element.multiplicity {
                if let Some(upper) = multiplicity.upper {
                    if multiplicity.lower > upper {
                        issues.push(ValidationIssue::new(
                            format!("{location}.multiplicity"),
                            format!(
                                "lower bound {} exceeds upper bound {upper}",
                                multiplicity.lower
                            ),
                        ));
                    }
                }
            }
            if element.kind.role() != Some(ElementRole::Usage)
                && (element.multiplicity.is_some()
                    || element.direction.is_some()
                    || element.ownership != UsageOwnership::Composite)
            {
                issues.push(ValidationIssue::new(
                    location.clone(),
                    "multiplicity, direction, and reference ownership apply only to usage elements",
                ));
            }
            if element.kind == ElementKind::AttributeUsage
                && element.ownership != UsageOwnership::Reference
            {
                issues.push(ValidationIssue::new(
                    location,
                    "attribute usages are always referential in SysML v2",
                ));
            }
        }

        for (index, relationship) in self.relationships.iter().enumerate() {
            let location = format!("relationships[{index}]");
            if relationship.id.trim().is_empty() {
                issues.push(ValidationIssue::new(
                    format!("{location}.id"),
                    "relationship id must not be empty",
                ));
            } else if !all_ids.insert(relationship.id.as_str()) {
                issues.push(ValidationIssue::new(
                    format!("{location}.id"),
                    format!("duplicate model id {:?}", relationship.id),
                ));
            }
            if relationship.sources.is_empty() {
                issues.push(ValidationIssue::new(
                    format!("{location}.sources"),
                    "relationship must have at least one source",
                ));
            }
            if relationship.targets.is_empty() {
                issues.push(ValidationIssue::new(
                    format!("{location}.targets"),
                    "relationship must have at least one target",
                ));
            }
            if relationship.kind.is_binary()
                && (relationship.sources.len() != 1 || relationship.targets.len() != 1)
            {
                issues.push(ValidationIssue::new(
                    location.clone(),
                    format!(
                        "{} relationships require exactly one source and one target",
                        relationship.kind.as_str()
                    ),
                ));
            }

            for (side, ids) in [
                ("sources", relationship.sources.as_slice()),
                ("targets", relationship.targets.as_slice()),
            ] {
                for id in ids {
                    if !elements.contains_key(id.as_str()) {
                        issues.push(ValidationIssue::new(
                            format!("{location}.{side}"),
                            format!("unknown element id {id:?}"),
                        ));
                    }
                }
            }

            validate_relationship_roles(relationship, &elements, &location, &mut issues);
        }

        validate_owners(&self.elements, &elements, &mut issues);

        if issues.is_empty() {
            Ok(())
        } else {
            Err(ValidationError { issues })
        }
    }
}

fn validate_relationship_roles(
    relationship: &Relationship,
    elements: &HashMap<&str, &Element>,
    location: &str,
    issues: &mut Vec<ValidationIssue>,
) {
    if relationship.sources.len() != 1 || relationship.targets.len() != 1 {
        return;
    }
    let Some(source) = elements.get(relationship.sources[0].as_str()) else {
        return;
    };
    let Some(target) = elements.get(relationship.targets[0].as_str()) else {
        return;
    };

    let role_error = match relationship.kind {
        RelationshipKind::FeatureTyping => {
            if source.kind.role() != Some(ElementRole::Usage)
                || target.kind.role() != Some(ElementRole::Definition)
            {
                Some("feature_typing must relate a usage to a definition")
            } else if source.kind.family() != target.kind.family() {
                Some("feature_typing must relate elements from the same concept family")
            } else {
                None
            }
        }
        RelationshipKind::Subclassification => {
            if source.kind.role() != Some(ElementRole::Definition)
                || target.kind.role() != Some(ElementRole::Definition)
            {
                Some("subclassification must relate two definitions")
            } else {
                None
            }
        }
        RelationshipKind::Subsetting | RelationshipKind::Redefinition => {
            if source.kind.role() != Some(ElementRole::Usage)
                || target.kind.role() != Some(ElementRole::Usage)
            {
                Some("subsetting and redefinition must relate two usages")
            } else {
                None
            }
        }
        RelationshipKind::Satisfaction | RelationshipKind::Verification => {
            if target.kind.family() != "requirement" {
                Some("satisfaction and verification must target a requirement")
            } else {
                None
            }
        }
        RelationshipKind::Include => {
            if source.kind.family() != "use_case" || target.kind.family() != "use_case" {
                Some("include must relate two use cases")
            } else {
                None
            }
        }
        _ => None,
    };

    if let Some(message) = role_error {
        issues.push(ValidationIssue::new(location, message));
    }
}

fn validate_owners(
    model_elements: &[Element],
    elements: &HashMap<&str, &Element>,
    issues: &mut Vec<ValidationIssue>,
) {
    for (index, element) in model_elements.iter().enumerate() {
        let Some(owner) = element.owner.as_deref() else {
            continue;
        };
        if owner == element.id {
            issues.push(ValidationIssue::new(
                format!("elements[{index}].owner"),
                "element cannot own itself",
            ));
            continue;
        }
        if !elements.contains_key(owner) {
            issues.push(ValidationIssue::new(
                format!("elements[{index}].owner"),
                format!("unknown owner id {owner:?}"),
            ));
            continue;
        }

        let mut visited = HashSet::from([element.id.as_str()]);
        let mut cursor = Some(owner);
        while let Some(id) = cursor {
            if !visited.insert(id) {
                issues.push(ValidationIssue::new(
                    format!("elements[{index}].owner"),
                    "ownership cycle detected",
                ));
                break;
            }
            cursor = elements
                .get(id)
                .and_then(|candidate| candidate.owner.as_deref());
        }
    }
}

/// Basic model counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelSummary {
    pub elements: usize,
    pub definitions: usize,
    pub usages: usize,
    pub relationships: usize,
}

/// One named modeling element.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Element {
    /// Stable identifier used by relationships and ownership references.
    pub id: String,
    /// Human-readable element name.
    pub name: String,
    /// SysML concept represented by this element.
    pub kind: ElementKind,
    /// Optional owning element identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// Optional cardinality for a usage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multiplicity: Option<Multiplicity>,
    /// Optional direction for a usage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<Direction>,
    /// Whether a usage is composite or referential.
    #[serde(default, skip_serializing_if = "UsageOwnership::is_composite")]
    pub ownership: UsageOwnership,
    /// Human-readable documentation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub documentation: Option<String>,
    /// Extension values kept as deterministic string pairs.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: BTreeMap<String, String>,
}

impl Element {
    /// Create an element with no owner, multiplicity, direction, or documentation.
    pub fn new(id: impl Into<String>, name: impl Into<String>, kind: ElementKind) -> Self {
        let ownership = if kind == ElementKind::AttributeUsage {
            UsageOwnership::Reference
        } else {
            UsageOwnership::Composite
        };
        Self {
            id: id.into(),
            name: name.into(),
            kind,
            owner: None,
            multiplicity: None,
            direction: None,
            ownership,
            documentation: None,
            properties: BTreeMap::new(),
        }
    }

    pub fn owned_by(mut self, owner: impl Into<String>) -> Self {
        self.owner = Some(owner.into());
        self
    }

    pub fn documented(mut self, documentation: impl Into<String>) -> Self {
        self.documentation = Some(documentation.into());
        self
    }

    pub fn with_multiplicity(mut self, multiplicity: Multiplicity) -> Self {
        self.multiplicity = Some(multiplicity);
        self
    }

    pub fn with_direction(mut self, direction: Direction) -> Self {
        self.direction = Some(direction);
        self
    }

    pub fn as_reference(mut self) -> Self {
        self.ownership = UsageOwnership::Reference;
        self
    }
}

/// Definition, usage, and supporting concepts from the major SysML v2 families.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ElementKind {
    Package,
    AttributeDefinition,
    AttributeUsage,
    EnumerationDefinition,
    EnumeratedValue,
    OccurrenceDefinition,
    OccurrenceUsage,
    IndividualDefinition,
    IndividualUsage,
    ItemDefinition,
    ItemUsage,
    PartDefinition,
    PartUsage,
    PortDefinition,
    PortUsage,
    ConnectionDefinition,
    ConnectionUsage,
    InterfaceDefinition,
    InterfaceUsage,
    AllocationDefinition,
    AllocationUsage,
    FlowDefinition,
    FlowUsage,
    ActionDefinition,
    ActionUsage,
    StateDefinition,
    StateUsage,
    TransitionUsage,
    CalculationDefinition,
    CalculationUsage,
    ConstraintDefinition,
    ConstraintUsage,
    RequirementDefinition,
    RequirementUsage,
    ConcernDefinition,
    ConcernUsage,
    CaseDefinition,
    CaseUsage,
    AnalysisCaseDefinition,
    AnalysisCaseUsage,
    VerificationCaseDefinition,
    VerificationCaseUsage,
    UseCaseDefinition,
    UseCaseUsage,
    ViewDefinition,
    ViewUsage,
    ViewpointDefinition,
    ViewpointUsage,
    RenderingDefinition,
    RenderingUsage,
    MetadataDefinition,
    MetadataUsage,
    Comment,
}

impl ElementKind {
    /// Whether this element is a definition, a usage, or a supporting element.
    pub const fn role(self) -> Option<ElementRole> {
        use ElementKind::*;
        match self {
            AttributeDefinition
            | EnumerationDefinition
            | OccurrenceDefinition
            | IndividualDefinition
            | ItemDefinition
            | PartDefinition
            | PortDefinition
            | ConnectionDefinition
            | InterfaceDefinition
            | AllocationDefinition
            | FlowDefinition
            | ActionDefinition
            | StateDefinition
            | CalculationDefinition
            | ConstraintDefinition
            | RequirementDefinition
            | ConcernDefinition
            | CaseDefinition
            | AnalysisCaseDefinition
            | VerificationCaseDefinition
            | UseCaseDefinition
            | ViewDefinition
            | ViewpointDefinition
            | RenderingDefinition
            | MetadataDefinition => Some(ElementRole::Definition),
            AttributeUsage
            | EnumeratedValue
            | OccurrenceUsage
            | IndividualUsage
            | ItemUsage
            | PartUsage
            | PortUsage
            | ConnectionUsage
            | InterfaceUsage
            | AllocationUsage
            | FlowUsage
            | ActionUsage
            | StateUsage
            | TransitionUsage
            | CalculationUsage
            | ConstraintUsage
            | RequirementUsage
            | ConcernUsage
            | CaseUsage
            | AnalysisCaseUsage
            | VerificationCaseUsage
            | UseCaseUsage
            | ViewUsage
            | ViewpointUsage
            | RenderingUsage
            | MetadataUsage => Some(ElementRole::Usage),
            Package | Comment => None,
        }
    }

    pub(crate) const fn family(self) -> &'static str {
        use ElementKind::*;
        match self {
            Package => "package",
            AttributeDefinition | AttributeUsage => "attribute",
            EnumerationDefinition | EnumeratedValue => "enumeration",
            OccurrenceDefinition | OccurrenceUsage => "occurrence",
            IndividualDefinition | IndividualUsage => "individual",
            ItemDefinition | ItemUsage => "item",
            PartDefinition | PartUsage => "part",
            PortDefinition | PortUsage => "port",
            ConnectionDefinition | ConnectionUsage => "connection",
            InterfaceDefinition | InterfaceUsage => "interface",
            AllocationDefinition | AllocationUsage => "allocation",
            FlowDefinition | FlowUsage => "flow",
            ActionDefinition | ActionUsage => "action",
            StateDefinition | StateUsage | TransitionUsage => "state",
            CalculationDefinition | CalculationUsage => "calculation",
            ConstraintDefinition | ConstraintUsage => "constraint",
            RequirementDefinition | RequirementUsage => "requirement",
            ConcernDefinition | ConcernUsage => "concern",
            CaseDefinition | CaseUsage => "case",
            AnalysisCaseDefinition | AnalysisCaseUsage => "analysis_case",
            VerificationCaseDefinition | VerificationCaseUsage => "verification_case",
            UseCaseDefinition | UseCaseUsage => "use_case",
            ViewDefinition | ViewUsage => "view",
            ViewpointDefinition | ViewpointUsage => "viewpoint",
            RenderingDefinition | RenderingUsage => "rendering",
            MetadataDefinition | MetadataUsage => "metadata",
            Comment => "comment",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementRole {
    Definition,
    Usage,
}

/// Cardinality for a usage. `None` as the upper bound means unbounded (`*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Multiplicity {
    pub lower: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upper: Option<u64>,
}

impl Multiplicity {
    pub const fn exactly(value: u64) -> Self {
        Self {
            lower: value,
            upper: Some(value),
        }
    }

    pub const fn bounded(lower: u64, upper: u64) -> Self {
        Self {
            lower,
            upper: Some(upper),
        }
    }

    pub const fn unbounded(lower: u64) -> Self {
        Self { lower, upper: None }
    }
}

/// Direction of values relative to the owning context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    In,
    Out,
    InOut,
}

/// Whether a usage composes its values or only references them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageOwnership {
    #[default]
    Composite,
    Reference,
}

impl UsageOwnership {
    fn is_composite(value: &Self) -> bool {
        *value == Self::Composite
    }
}

/// A typed directed relationship between model elements.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Relationship {
    pub id: String,
    pub kind: RelationshipKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub sources: Vec<String>,
    pub targets: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub documentation: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: BTreeMap<String, String>,
}

impl Relationship {
    /// Create a binary relationship.
    pub fn new(
        id: impl Into<String>,
        kind: RelationshipKind,
        source: impl Into<String>,
        target: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            kind,
            name: None,
            sources: vec![source.into()],
            targets: vec![target.into()],
            documentation: None,
            properties: BTreeMap::new(),
        }
    }

    /// Create a relationship with arbitrary source and target arity.
    pub fn nary(
        id: impl Into<String>,
        kind: RelationshipKind,
        sources: impl IntoIterator<Item = impl Into<String>>,
        targets: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            id: id.into(),
            kind,
            name: None,
            sources: sources.into_iter().map(Into::into).collect(),
            targets: targets.into_iter().map(Into::into).collect(),
            documentation: None,
            properties: BTreeMap::new(),
        }
    }

    pub fn named(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
}

/// Major relationship families used to construct and connect SysML models.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum RelationshipKind {
    Ownership,
    Dependency,
    Annotation,
    FeatureTyping,
    Subclassification,
    Subsetting,
    Redefinition,
    Connection,
    Binding,
    Succession,
    Interface,
    Allocation,
    Flow,
    Transition,
    Satisfaction,
    Verification,
    Include,
    Expose,
    Rendering,
}

impl RelationshipKind {
    const fn is_binary(self) -> bool {
        !matches!(self, Self::Dependency | Self::Annotation | Self::Connection)
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Ownership => "ownership",
            Self::Dependency => "dependency",
            Self::Annotation => "annotation",
            Self::FeatureTyping => "feature_typing",
            Self::Subclassification => "subclassification",
            Self::Subsetting => "subsetting",
            Self::Redefinition => "redefinition",
            Self::Connection => "connection",
            Self::Binding => "binding",
            Self::Succession => "succession",
            Self::Interface => "interface",
            Self::Allocation => "allocation",
            Self::Flow => "flow",
            Self::Transition => "transition",
            Self::Satisfaction => "satisfaction",
            Self::Verification => "verification",
            Self::Include => "include",
            Self::Expose => "expose",
            Self::Rendering => "rendering",
        }
    }
}
