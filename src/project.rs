use crate::check::CheckError;
use std::path::PathBuf;
use tree_sitter::{Node, Parser};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct SourceId(pub(crate) usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ElementId(pub(crate) usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Position {
    pub(crate) byte: usize,
    pub(crate) line: usize,
    pub(crate) column: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Span {
    pub(crate) source: SourceId,
    pub(crate) start: Position,
    pub(crate) end: Position,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Reference {
    pub(crate) text: String,
    pub(crate) segments: Vec<String>,
    pub(crate) form: ReferenceForm,
    pub(crate) span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReferenceForm {
    Qualified,
    FeatureChain,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Documentation {
    pub(crate) text: String,
    pub(crate) span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Source {
    pub(crate) id: SourceId,
    pub(crate) path: String,
    pub(crate) text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ElementKind {
    Package,
    PartDefinition,
    PartUsage,
    RequirementDefinition,
    RequirementUsage,
    ConstraintDefinition,
    ConstraintUsage,
    VerificationDefinition,
    VerificationUsage,
    Objective,
    Subject,
    Actor,
    Stakeholder,
    GenericDefinition,
    GenericUsage,
}

impl ElementKind {
    pub(crate) fn is_usage(self) -> bool {
        matches!(
            self,
            Self::PartUsage
                | Self::RequirementUsage
                | Self::ConstraintUsage
                | Self::VerificationUsage
                | Self::Objective
                | Self::Subject
                | Self::Actor
                | Self::Stakeholder
                | Self::GenericUsage
        )
    }

    pub(crate) fn is_requirement_usage(self) -> bool {
        matches!(self, Self::RequirementUsage | Self::Objective)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Element {
    pub(crate) id: ElementId,
    pub(crate) kind: ElementKind,
    pub(crate) name: Option<String>,
    pub(crate) short_name: Option<String>,
    pub(crate) owner: Option<ElementId>,
    pub(crate) package: Option<ElementId>,
    pub(crate) span: Span,
    pub(crate) declared_types: Vec<Reference>,
    pub(crate) specializations: Vec<Reference>,
    pub(crate) documentation: Vec<Documentation>,
    pub(crate) referenced_feature: Option<Reference>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Import {
    pub(crate) scope: Option<ElementId>,
    pub(crate) visibility: ImportVisibility,
    pub(crate) reference: Reference,
    pub(crate) all: bool,
    pub(crate) wildcard: bool,
    pub(crate) recursive: bool,
    pub(crate) filtered: bool,
    pub(crate) span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ImportVisibility {
    Missing,
    Private,
    Protected,
    Public,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Requirement {
    pub(crate) element: ElementId,
    pub(crate) subjects: Vec<ElementId>,
    pub(crate) actors: Vec<ElementId>,
    pub(crate) stakeholders: Vec<ElementId>,
    pub(crate) assumptions: Vec<ElementId>,
    pub(crate) required_constraints: Vec<ElementId>,
    pub(crate) nested_requirements: Vec<ElementId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Satisfaction {
    pub(crate) package: Option<ElementId>,
    pub(crate) owner: Option<ElementId>,
    pub(crate) requirement: Option<Reference>,
    pub(crate) inline_requirement: Option<ElementId>,
    pub(crate) subject: Option<Reference>,
    pub(crate) explicitly_asserted: bool,
    pub(crate) negated: bool,
    pub(crate) span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Verification {
    pub(crate) element: ElementId,
    pub(crate) subjects: Vec<ElementId>,
    pub(crate) objectives: Vec<ElementId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VerifyAssertion {
    pub(crate) owner: ElementId,
    pub(crate) package: Option<ElementId>,
    pub(crate) target: Option<Reference>,
    pub(crate) inline_requirement: Option<ElementId>,
    pub(crate) span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProfileIssue {
    pub(crate) span: Span,
    pub(crate) code: &'static str,
    pub(crate) message: &'static str,
}

#[derive(Debug, Default)]
pub(crate) struct Project {
    pub(crate) sources: Vec<Source>,
    pub(crate) elements: Vec<Element>,
    pub(crate) imports: Vec<Import>,
    pub(crate) requirements: Vec<Requirement>,
    pub(crate) verifications: Vec<Verification>,
    pub(crate) satisfactions: Vec<Satisfaction>,
    pub(crate) verify_assertions: Vec<VerifyAssertion>,
    pub(crate) profile_issues: Vec<ProfileIssue>,
}

impl Project {
    pub(crate) fn from_sources(mut sources: Vec<(PathBuf, String)>) -> Result<Self, CheckError> {
        sources.sort_by(|left, right| left.0.cmp(&right.0));
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_sysml::LANGUAGE.into())
            .map_err(|error| CheckError::ParserInitialization(error.to_string()))?;

        let mut project = Self::default();
        for (index, (path, text)) in sources.into_iter().enumerate() {
            let source_id = SourceId(index);
            let tree = parser.parse(&text, None).ok_or_else(|| {
                CheckError::ParserInitialization(format!(
                    "the parser did not produce a syntax tree for {}",
                    path.display()
                ))
            })?;
            project.sources.push(Source {
                id: source_id,
                path: normalize_path(&path),
                text: text.clone(),
            });
            let mut lowerer = Lowerer {
                project: &mut project,
                source_id,
                source: &text,
            };
            lowerer.lower_children(tree.root_node(), Context::default());
        }
        Ok(project)
    }

    pub(crate) fn source(&self, id: SourceId) -> &Source {
        &self.sources[id.0]
    }

    pub(crate) fn element(&self, id: ElementId) -> &Element {
        &self.elements[id.0]
    }

    fn requirement_mut(&mut self, id: ElementId) -> Option<&mut Requirement> {
        self.requirements
            .iter_mut()
            .find(|requirement| requirement.element == id)
    }

    fn verification_mut(&mut self, id: ElementId) -> Option<&mut Verification> {
        self.verifications
            .iter_mut()
            .find(|verification| verification.element == id)
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Context {
    owner: Option<ElementId>,
    package: Option<ElementId>,
    requirement: Option<ElementId>,
    verification: Option<ElementId>,
    objective: Option<ElementId>,
}

struct Lowerer<'a> {
    project: &'a mut Project,
    source_id: SourceId,
    source: &'a str,
}

impl Lowerer<'_> {
    fn lower_children(&mut self, node: Node<'_>, context: Context) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.lower_node(child, context);
        }
    }

    fn lower_node(&mut self, node: Node<'_>, context: Context) {
        match node.kind() {
            "package_definition" | "library_package" => self.lower_package(node, context),
            "import_statement" => self.lower_import(node, context),
            "requirement_definition" => {
                self.lower_requirement(node, context, ElementKind::RequirementDefinition)
            }
            "requirement_usage" => {
                self.lower_requirement(node, context, ElementKind::RequirementUsage)
            }
            "verification_definition" => {
                self.lower_verification(node, context, ElementKind::VerificationDefinition)
            }
            "verification_usage" => {
                self.lower_verification(node, context, ElementKind::VerificationUsage)
            }
            "objective_usage" => self.lower_objective(node, context),
            "subject_statement" => self.lower_parameter(node, context, ElementKind::Subject),
            "actor_usage" => self.lower_parameter(node, context, ElementKind::Actor),
            "stakeholder_usage" => self.lower_parameter(node, context, ElementKind::Stakeholder),
            "assume_statement" => self.lower_constraint_member(node, context, true),
            "require_statement" => self.lower_constraint_member(node, context, false),
            "satisfy_statement" => self.lower_satisfaction(node, context),
            "assert_statement" if has_direct_token(node, "satisfy") => {
                self.lower_satisfaction(node, context)
            }
            "verify_statement" => self.lower_verify(node, context),
            "shorthand_attribute" if has_direct_token(node, ":>") => {
                self.lower_element(node, context, ElementKind::GenericUsage)
            }
            kind if element_kind(kind).is_some() => {
                self.lower_element(node, context, element_kind(kind).expect("kind was checked"));
            }
            // An otherwise-unmodeled construct with a member body owns those
            // members. Keep a generic ownership barrier so nested profile
            // elements are not attributed to an enclosing requirement,
            // verification, or objective.
            _ if owns_member_body(node) => self.lower_unmodeled_owner(node, context),
            _ => self.lower_children(node, context),
        }
    }

    fn lower_package(&mut self, node: Node<'_>, context: Context) {
        let id = self.add_element(node, ElementKind::Package, context, None);
        let package_context = Context {
            owner: Some(id),
            package: Some(id),
            requirement: None,
            verification: None,
            objective: None,
        };
        self.lower_children(node, package_context);
    }

    fn lower_import(&mut self, node: Node<'_>, context: Context) {
        let visibility = direct_child(node, "visibility")
            .map(|visibility| match &self.source[visibility.byte_range()] {
                "private" => ImportVisibility::Private,
                "protected" => ImportVisibility::Protected,
                "public" => ImportVisibility::Public,
                _ => ImportVisibility::Missing,
            })
            .unwrap_or(ImportVisibility::Missing);
        let all = has_direct_token(node, "all");
        let wildcard_node = descendant(node, "wildcard_import");
        let wildcard = wildcard_node.is_some();
        let recursive = wildcard_node.is_some_and(|wildcard| has_direct_token(wildcard, "**"));
        let filtered = wildcard_node.is_some_and(|wildcard| has_direct_token(wildcard, "["));
        let reference_node = wildcard_node
            .or_else(|| descendant(node, "qualified_name"))
            .or_else(|| descendant(node, "name"));
        if let Some(reference_node) = reference_node {
            self.project.imports.push(Import {
                scope: context.owner,
                visibility,
                reference: model_reference(reference_node, self.source_id, self.source),
                all,
                wildcard,
                recursive,
                filtered,
                span: self.span(node),
            });
        }
    }

    fn lower_requirement(&mut self, node: Node<'_>, context: Context, kind: ElementKind) {
        let id = self.add_requirement_element(node, kind, context);
        let next = Context {
            owner: Some(id),
            requirement: Some(id),
            verification: None,
            objective: None,
            ..context
        };
        self.lower_children(node, next);
    }

    fn add_requirement_element(
        &mut self,
        node: Node<'_>,
        kind: ElementKind,
        context: Context,
    ) -> ElementId {
        debug_assert!(kind == ElementKind::RequirementDefinition || kind.is_requirement_usage());
        let id = self.add_element(node, kind, context, None);
        if let Some(parent) = context
            .requirement
            .filter(|parent| context.owner == Some(*parent))
        {
            if let Some(requirement) = self.project.requirement_mut(parent) {
                requirement.nested_requirements.push(id);
            }
        }
        self.project.requirements.push(Requirement {
            element: id,
            subjects: Vec::new(),
            actors: Vec::new(),
            stakeholders: Vec::new(),
            assumptions: Vec::new(),
            required_constraints: Vec::new(),
            nested_requirements: Vec::new(),
        });
        id
    }

    fn lower_verification(&mut self, node: Node<'_>, context: Context, kind: ElementKind) {
        let id = self.add_element(node, kind, context, None);
        self.project.verifications.push(Verification {
            element: id,
            subjects: Vec::new(),
            objectives: Vec::new(),
        });
        let next = Context {
            owner: Some(id),
            requirement: None,
            verification: Some(id),
            objective: None,
            ..context
        };
        self.lower_children(node, next);
    }

    fn lower_objective(&mut self, node: Node<'_>, context: Context) {
        let id = self.add_requirement_element(node, ElementKind::Objective, context);
        if let Some(verification) = context
            .verification
            .filter(|verification| context.owner == Some(*verification))
        {
            if let Some(record) = self.project.verification_mut(verification) {
                record.objectives.push(id);
            }
        }
        let next = Context {
            owner: Some(id),
            requirement: Some(id),
            objective: Some(id),
            ..context
        };
        self.lower_children(node, next);
    }

    fn lower_parameter(&mut self, node: Node<'_>, context: Context, kind: ElementKind) {
        let referenced_feature = parameter_binding(node, self.source_id, self.source);
        let id = self.add_element(node, kind, context, referenced_feature);
        if let Some(requirement) = context
            .requirement
            .filter(|requirement| context.owner == Some(*requirement))
        {
            if let Some(record) = self.project.requirement_mut(requirement) {
                match kind {
                    ElementKind::Subject => record.subjects.push(id),
                    ElementKind::Actor => record.actors.push(id),
                    ElementKind::Stakeholder => record.stakeholders.push(id),
                    _ => {}
                }
            }
        }
        if kind == ElementKind::Subject {
            if let Some(verification) = context
                .verification
                .filter(|verification| context.owner == Some(*verification))
            {
                if context.objective.is_none() {
                    if let Some(record) = self.project.verification_mut(verification) {
                        record.subjects.push(id);
                    }
                }
            }
        }
        self.lower_children(
            node,
            Context {
                owner: Some(id),
                requirement: None,
                verification: None,
                objective: None,
                ..context
            },
        );
    }

    fn lower_constraint_member(&mut self, node: Node<'_>, context: Context, assumption: bool) {
        let referenced_feature = direct_child(node, "feature_chain")
            .or_else(|| direct_child(node, "qualified_name"))
            .map(|node| model_reference(node, self.source_id, self.source));
        let id = self.add_element(
            node,
            ElementKind::ConstraintUsage,
            context,
            referenced_feature,
        );
        if let Some(requirement) = context
            .requirement
            .filter(|requirement| context.owner == Some(*requirement))
        {
            if let Some(record) = self.project.requirement_mut(requirement) {
                if assumption {
                    record.assumptions.push(id);
                } else {
                    record.required_constraints.push(id);
                }
            }
        }
        self.lower_children(
            node,
            Context {
                owner: Some(id),
                requirement: None,
                verification: None,
                objective: None,
                ..context
            },
        );
    }

    fn lower_satisfaction(&mut self, node: Node<'_>, context: Context) {
        let explicitly_asserted = node.kind() == "assert_statement";
        let negated = has_direct_token(node, "not");
        let inline = direct_child(node, "usage_declaration").is_some();
        if !inline && has_direct_token(node, "{") {
            self.project.profile_issues.push(ProfileIssue {
                span: self.span(node),
                code: "semantic.profile.unsupported_relationship_body",
                message:
                    "bodies on referenced satisfaction usages are outside this validation profile",
            });
        }
        if inline {
            let requirement =
                self.add_requirement_element(node, ElementKind::RequirementUsage, context);
            let subject = direct_child(node, "feature_chain")
                .map(|node| model_reference(node, self.source_id, self.source));
            self.project.satisfactions.push(Satisfaction {
                package: context.package,
                owner: context.owner,
                requirement: None,
                inline_requirement: Some(requirement),
                subject,
                explicitly_asserted,
                negated,
                span: self.span(node),
            });
            self.lower_children(
                node,
                Context {
                    owner: Some(requirement),
                    requirement: Some(requirement),
                    verification: None,
                    objective: None,
                    ..context
                },
            );
            return;
        }

        let requirement_node =
            direct_child(node, "qualified_name").or_else(|| direct_child(node, "feature_chain"));
        let feature_nodes = direct_children(node, "feature_chain");
        let subject_node = if direct_child(node, "qualified_name").is_some() {
            feature_nodes.first().copied()
        } else {
            feature_nodes.get(1).copied()
        };
        if let Some(requirement_node) = requirement_node {
            self.project.satisfactions.push(Satisfaction {
                package: context.package,
                owner: context.owner,
                requirement: Some(model_reference(
                    requirement_node,
                    self.source_id,
                    self.source,
                )),
                inline_requirement: None,
                subject: subject_node
                    .map(|node| model_reference(node, self.source_id, self.source)),
                explicitly_asserted,
                negated,
                span: self.span(node),
            });
        }
    }

    fn lower_verify(&mut self, node: Node<'_>, context: Context) {
        if direct_child(node, "feature_chain").is_some() && has_direct_token(node, "{") {
            self.project.profile_issues.push(ProfileIssue {
                span: self.span(node),
                code: "semantic.profile.unsupported_relationship_body",
                message:
                    "bodies on referenced verification usages are outside this validation profile",
            });
        }
        let inline = has_direct_token(node, "requirement");
        let inline_requirement = inline
            .then(|| self.add_requirement_element(node, ElementKind::RequirementUsage, context));
        let target_node = (!inline)
            .then(|| {
                direct_child(node, "feature_chain").or_else(|| direct_child(node, "qualified_name"))
            })
            .flatten();
        if let Some(owner) = context.objective.or(context.owner) {
            self.project.verify_assertions.push(VerifyAssertion {
                owner,
                package: context.package,
                target: target_node.map(|node| model_reference(node, self.source_id, self.source)),
                inline_requirement,
                span: self.span(node),
            });
        }
    }

    fn lower_unmodeled_owner(&mut self, node: Node<'_>, context: Context) {
        let id = self.add_element(node, ElementKind::GenericUsage, context, None);
        self.lower_children(
            node,
            Context {
                owner: Some(id),
                requirement: None,
                verification: None,
                objective: None,
                ..context
            },
        );
    }

    fn lower_element(&mut self, node: Node<'_>, context: Context, kind: ElementKind) {
        let id = self.add_element(node, kind, context, None);
        self.lower_children(
            node,
            Context {
                owner: Some(id),
                requirement: None,
                verification: None,
                objective: None,
                ..context
            },
        );
    }

    fn add_element(
        &mut self,
        node: Node<'_>,
        kind: ElementKind,
        context: Context,
        referenced_feature: Option<Reference>,
    ) -> ElementId {
        if let Some(visibility) = direct_child(node, "visibility").filter(|visibility| {
            matches!(
                &self.source[visibility.byte_range()],
                "private" | "protected"
            )
        }) {
            self.project.profile_issues.push(ProfileIssue {
                span: self.span(visibility),
                code: "semantic.profile.unsupported_visibility",
                message: "private and protected visibility on non-import memberships is outside this validation profile",
            });
        }
        let id = ElementId(self.project.elements.len());
        let identification = declaration(node)
            .and_then(|declaration| direct_child(declaration, "identification"))
            .or_else(|| direct_child(node, "identification"));
        let (short_name, name) = if node.kind() == "shorthand_attribute" {
            (
                None,
                direct_child(node, "name")
                    .map(|name| canonical_name(&self.source[name.byte_range()])),
            )
        } else {
            identification
                .map(|identification| identification_names(identification, self.source))
                .unwrap_or_default()
        };
        let element = Element {
            id,
            kind,
            name,
            short_name,
            owner: context.owner,
            package: context.package,
            span: self.span(node),
            declared_types: declaration_types(node, self.source_id, self.source),
            specializations: specialization_types(node, self.source_id, self.source),
            documentation: documentation(node, self.source_id, self.source),
            referenced_feature,
        };
        self.project.elements.push(element);
        id
    }

    fn span(&self, node: Node<'_>) -> Span {
        span(self.source_id, node)
    }
}

fn element_kind(kind: &str) -> Option<ElementKind> {
    match kind {
        "part_definition" => Some(ElementKind::PartDefinition),
        "part_usage" => Some(ElementKind::PartUsage),
        "constraint_definition" => Some(ElementKind::ConstraintDefinition),
        "constraint_usage" => Some(ElementKind::ConstraintUsage),
        kind if kind.ends_with("_definition") => Some(ElementKind::GenericDefinition),
        kind if kind.ends_with("_usage") => Some(ElementKind::GenericUsage),
        _ => None,
    }
}

fn declaration(node: Node<'_>) -> Option<Node<'_>> {
    direct_child(node, "usage_declaration")
        .or_else(|| direct_child(node, "definition_declaration"))
        .or_else(|| {
            direct_child(node, "usage").and_then(|usage| direct_child(usage, "usage_declaration"))
        })
        .or_else(|| {
            direct_child(node, "definition")
                .and_then(|definition| direct_child(definition, "definition_declaration"))
        })
}

fn identification_names(node: Node<'_>, source: &str) -> (Option<String>, Option<String>) {
    let names = direct_children(node, "name");
    let explicit_id = has_direct_token(node, "id");
    let short_name = direct_child(node, "short_name")
        .map(|name| canonical_short_name(&source[name.byte_range()]))
        .or_else(|| {
            direct_child(node, "string_literal")
                .map(|name| canonical_string_literal(&source[name.byte_range()]))
        })
        .or_else(|| {
            explicit_id
                .then(|| names.first())
                .flatten()
                .map(|name| canonical_name(&source[name.byte_range()]))
        });
    let name = if explicit_id {
        let index = usize::from(direct_child(node, "string_literal").is_none());
        names
            .get(index)
            .map(|name| canonical_name(&source[name.byte_range()]))
    } else {
        names
            .last()
            .map(|name| canonical_name(&source[name.byte_range()]))
    };
    (short_name, name)
}

fn declaration_types(node: Node<'_>, source_id: SourceId, source: &str) -> Vec<Reference> {
    let root = declaration(node).unwrap_or(node);
    let Some(typing) = direct_child(root, "typing_part") else {
        return Vec::new();
    };
    let mut references = Vec::new();
    collect_descendants(typing, "qualified_name", &mut |reference| {
        references.push(model_reference(reference, source_id, source));
    });
    references
}

fn specialization_types(node: Node<'_>, source_id: SourceId, source: &str) -> Vec<Reference> {
    let mut references = Vec::new();
    if let Some(specialization) = direct_child(node, "definition_specialization")
        .or_else(|| direct_child(node, "specialization_part"))
        .or_else(|| {
            (node.kind() == "shorthand_attribute" && has_direct_token(node, ":>"))
                .then(|| direct_child(node, "qualified_name"))
                .flatten()
        })
    {
        collect_reference_nodes(specialization, &mut |reference| {
            references.push(model_reference(reference, source_id, source));
        });
    } else if let Some(declaration) = declaration(node) {
        collect_descendants(declaration, "specialization_part", &mut |specialization| {
            collect_reference_nodes(specialization, &mut |reference| {
                references.push(model_reference(reference, source_id, source));
            });
        });
    }
    references
}

fn collect_reference_nodes<'tree>(node: Node<'tree>, visit: &mut impl FnMut(Node<'tree>)) {
    if matches!(node.kind(), "qualified_name" | "feature_chain") {
        visit(node);
        return;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_reference_nodes(child, visit);
    }
}

fn documentation(node: Node<'_>, source_id: SourceId, source: &str) -> Vec<Documentation> {
    let mut items = Vec::new();
    collect_owned_documentation(node, node, source_id, source, &mut items);
    items
}

fn collect_owned_documentation(
    root: Node<'_>,
    node: Node<'_>,
    source_id: SourceId,
    source: &str,
    items: &mut Vec<Documentation>,
) {
    if node != root && owns_independent_documentation(node) {
        return;
    }
    if node.kind() == "documentation" {
        let body = direct_child(node, "block_comment_body");
        if let Some(body) = body {
            items.push(Documentation {
                text: documentation_text(&source[body.byte_range()]),
                span: span(source_id, body),
            });
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_owned_documentation(root, child, source_id, source, items);
    }
}

fn owns_independent_documentation(node: Node<'_>) -> bool {
    let kind = node.kind();
    matches!(
        kind,
        "requirement_definition"
            | "requirement_usage"
            | "verification_definition"
            | "verification_usage"
            | "objective_usage"
            | "satisfy_statement"
            | "verify_statement"
            | "package_definition"
            | "library_package"
            | "subject_statement"
            | "actor_usage"
            | "stakeholder_usage"
            | "assume_statement"
            | "require_statement"
            | "perform_statement"
    ) || kind.ends_with("_statement")
        || element_kind(kind).is_some()
        || owns_member_body(node)
}

fn owns_member_body(node: Node<'_>) -> bool {
    let mut cursor = node.walk();
    let has_named_member_body = node.named_children(&mut cursor).any(|child| {
        matches!(
            child.kind(),
            "action_body"
                | "constraint_body"
                | "definition_body"
                | "package_body"
                | "requirement_body"
                | "state_body"
                | "structural_body"
                | "usage_body"
        ) && has_direct_token(child, "{")
    });
    has_named_member_body
        || (has_direct_token(node, "{")
            && (node.kind().ends_with("_statement")
                || matches!(
                    node.kind(),
                    "then_if_block"
                        | "if_else_block"
                        | "control_node"
                        | "then_succession"
                        | "for_loop"
                        | "while_loop"
                        | "loop_until"
                )))
}

fn parameter_binding(node: Node<'_>, source_id: SourceId, source: &str) -> Option<Reference> {
    let declaration = declaration(node)?;
    let value = direct_child(declaration, "value_part")?;
    let reference = direct_child(value, "feature_chain")
        .or_else(|| direct_child(value, "qualified_name"))
        .or_else(|| direct_child(value, "feature_chain_expression"))?;
    pure_reference(reference, source_id, source)
}

fn pure_reference(node: Node<'_>, source_id: SourceId, source: &str) -> Option<Reference> {
    if matches!(node.kind(), "feature_chain" | "qualified_name") {
        return Some(model_reference(node, source_id, source));
    }
    if node.kind() != "feature_chain_expression" {
        return None;
    }

    let mut cursor = node.walk();
    let mut children = node.named_children(&mut cursor);
    let base = children.next()?;
    let member = children.next()?;
    if children.next().is_some() || member.kind() != "name" {
        return None;
    }

    let mut reference = pure_reference(base, source_id, source)?;
    let member = canonical_name(&source[member.byte_range()]);
    reference.text.push('.');
    reference.text.push_str(&member);
    reference.segments.push(member);
    reference.form = ReferenceForm::FeatureChain;
    reference.span = span(source_id, node);
    Some(reference)
}

fn model_reference(node: Node<'_>, source_id: SourceId, source: &str) -> Reference {
    let mut segments = Vec::new();
    collect_descendants(node, "name", &mut |name| {
        segments.push(canonical_name(&source[name.byte_range()]));
    });
    if segments.is_empty() {
        let fallback = source[node.byte_range()]
            .trim()
            .strip_suffix("::*")
            .unwrap_or(source[node.byte_range()].trim());
        segments.push(canonical_name(fallback));
    }
    let form = if node.kind() == "feature_chain" {
        ReferenceForm::FeatureChain
    } else {
        ReferenceForm::Qualified
    };
    let separator = match form {
        ReferenceForm::Qualified => "::",
        ReferenceForm::FeatureChain => ".",
    };
    Reference {
        text: segments.join(separator),
        segments,
        form,
        span: span(source_id, node),
    }
}

fn documentation_text(value: &str) -> String {
    let value = value.trim();
    value
        .strip_prefix("/*")
        .and_then(|value| value.strip_suffix("*/"))
        .unwrap_or(value)
        .trim()
        .to_owned()
}

fn span(source: SourceId, node: Node<'_>) -> Span {
    let start = node.start_position();
    let end = node.end_position();
    Span {
        source,
        start: Position {
            byte: node.start_byte(),
            line: start.row + 1,
            column: start.column + 1,
        },
        end: Position {
            byte: node.end_byte(),
            line: end.row + 1,
            column: end.column + 1,
        },
    }
}

fn direct_child<'tree>(node: Node<'tree>, kind: &str) -> Option<Node<'tree>> {
    let mut cursor = node.walk();
    let child = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == kind);
    child
}

fn direct_children<'tree>(node: Node<'tree>, kind: &str) -> Vec<Node<'tree>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|child| child.kind() == kind)
        .collect()
}

fn has_direct_token(node: Node<'_>, token: &str) -> bool {
    let mut cursor = node.walk();
    let found = node
        .children(&mut cursor)
        .any(|child| child.kind() == token);
    found
}

fn descendant<'tree>(node: Node<'tree>, kind: &str) -> Option<Node<'tree>> {
    if node.kind() == kind {
        return Some(node);
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if let Some(found) = descendant(child, kind) {
            return Some(found);
        }
    }
    None
}

fn collect_descendants<'tree>(node: Node<'tree>, kind: &str, visit: &mut impl FnMut(Node<'tree>)) {
    if node.kind() == kind {
        visit(node);
        return;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_descendants(child, kind, visit);
    }
}

fn canonical_short_name(value: &str) -> String {
    let value = value.trim();
    let inner = value
        .strip_prefix('<')
        .and_then(|value| value.strip_suffix('>'))
        .unwrap_or(value);
    canonical_name(inner)
}

fn canonical_string_literal(value: &str) -> String {
    let value = value.trim();
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(value)
        .to_owned()
}

fn canonical_name(value: &str) -> String {
    let value = value.trim();
    value
        .strip_prefix('\'')
        .and_then(|value| value.strip_suffix('\''))
        .unwrap_or(value)
        .to_owned()
}

fn normalize_path(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowers_requirement_identity_text_subject_and_relationships() {
        let project = Project::from_sources(vec![(
            PathBuf::from("requirements.sysml"),
            r#"
package Requirements {
    part def System;
    part def System;
    part system : System;
    requirement def <'REQ-1'> SystemRequirement {
        doc /* The system shall work. */
        subject subjectSystem : System;
        assume constraint { true }
        require constraint { true }
    }
    requirement selected : SystemRequirement;
    part context { satisfy selected by system; }
    assert not satisfy selected by system;
    verification def Test {
        objective { verify selected; }
    }
}
"#
            .to_owned(),
        )])
        .expect("project should lower");

        assert_eq!(project.sources.len(), 1);
        assert_eq!(project.requirements.len(), 3);
        let definition = project.element(project.requirements[0].element);
        assert_eq!(definition.short_name.as_deref(), Some("REQ-1"));
        assert_eq!(definition.name.as_deref(), Some("SystemRequirement"));
        assert_eq!(definition.documentation[0].text, "The system shall work.");
        assert_eq!(project.requirements[0].subjects.len(), 1);
        assert_eq!(project.requirements[0].assumptions.len(), 1);
        assert_eq!(project.requirements[0].required_constraints.len(), 1);
        assert_eq!(project.satisfactions.len(), 2);
        assert!(!project.satisfactions[0].explicitly_asserted);
        assert!(!project.satisfactions[0].negated);
        assert!(project.satisfactions[1].explicitly_asserted);
        assert!(project.satisfactions[1].negated);
        assert_eq!(project.verify_assertions.len(), 1);
    }

    #[test]
    fn keeps_nested_requirement_metadata_with_its_declaring_element() {
        let project = Project::from_sources(vec![(
            PathBuf::from("nested.sysml"),
            r#"
package Requirements {
    requirement def id "REQ-BASE" Base;
    requirement def id REQ_ONE;
    requirement def id REQ_TWO NamedRequirement;
    requirement def <REQ_THREE> AngleRequirement;
    requirement def Outer {
        doc locale "en"
        doc /* Outer text. */
        subject unit : System {
            doc /* Subject text. */
        }
        requirement def Inner :> Base {
            doc /* Inner text. */
        }
    }
}
"#
            .to_owned(),
        )])
        .expect("project should lower");

        let outer = project
            .elements
            .iter()
            .find(|element| element.name.as_deref() == Some("Outer"))
            .expect("outer requirement should exist");
        let inner = project
            .elements
            .iter()
            .find(|element| element.name.as_deref() == Some("Inner"))
            .expect("inner requirement should exist");
        let base = project
            .elements
            .iter()
            .find(|element| element.name.as_deref() == Some("Base"))
            .expect("base requirement should exist");
        assert_eq!(base.short_name.as_deref(), Some("REQ-BASE"));
        let id_only = project
            .elements
            .iter()
            .find(|element| element.short_name.as_deref() == Some("REQ_ONE"))
            .expect("identifier-only requirement should exist");
        assert_eq!(id_only.name, None);
        let id_and_name = project
            .elements
            .iter()
            .find(|element| element.short_name.as_deref() == Some("REQ_TWO"))
            .expect("identifier-and-name requirement should exist");
        assert_eq!(id_and_name.name.as_deref(), Some("NamedRequirement"));
        let angle_id = project
            .elements
            .iter()
            .find(|element| element.name.as_deref() == Some("AngleRequirement"))
            .expect("angle-bracket identifier requirement should exist");
        assert_eq!(angle_id.short_name.as_deref(), Some("REQ_THREE"));
        assert_eq!(outer.documentation.len(), 1);
        assert_eq!(outer.documentation[0].text, "Outer text.");
        assert!(outer.specializations.is_empty());
        assert_eq!(inner.documentation[0].text, "Inner text.");
        assert_eq!(inner.specializations[0].text, "Base");
        let outer_requirement = project
            .requirements
            .iter()
            .find(|requirement| requirement.element == outer.id)
            .expect("outer requirement record should exist");
        let subject = project.element(outer_requirement.subjects[0]);
        assert_eq!(subject.documentation[0].text, "Subject text.");
    }

    #[test]
    fn lowers_membership_imports_and_subject_bindings() {
        let project = Project::from_sources(vec![(
            PathBuf::from("imports.sysml"),
            r#"
package Architecture {
    part def Computer;
    part computer : Computer;
}
package Requirements {
    private import Architecture::Computer;
    private import Architecture::*;
    verification check {
        subject tested = computer;
    }
}
"#
            .to_owned(),
        )])
        .expect("project should lower");

        assert_eq!(project.imports.len(), 2);
        assert_eq!(project.imports[0].visibility, ImportVisibility::Private);
        assert_eq!(project.imports[0].reference.text, "Architecture::Computer");
        assert!(!project.imports[0].wildcard);
        assert_eq!(project.imports[1].reference.text, "Architecture");
        assert!(project.imports[1].wildcard);
        let subject = project
            .elements
            .iter()
            .find(|element| element.name.as_deref() == Some("tested"))
            .expect("verification subject should exist");
        assert_eq!(
            subject
                .referenced_feature
                .as_ref()
                .map(|binding| binding.text.as_str()),
            Some("computer")
        );
    }

    #[test]
    fn distinguishes_import_all_from_ordinary_and_wildcard_imports() {
        let project = Project::from_sources(vec![(
            PathBuf::from("import-all.sysml"),
            r#"
package Model {
    import Model::member;
    import all Model::member;
    import Model::*;
    import all Model::*;
}
"#
            .to_owned(),
        )])
        .expect("project should lower");

        assert_eq!(project.imports.len(), 4);
        assert_eq!(
            project
                .imports
                .iter()
                .map(|import| (import.all, import.wildcard))
                .collect::<Vec<_>>(),
            [(false, false), (true, false), (false, true), (true, true)]
        );
        assert_eq!(project.imports[0].reference.text, "Model::member");
        assert_eq!(project.imports[1].reference.text, "Model::member");
        assert_eq!(project.imports[2].reference.text, "Model");
        assert_eq!(project.imports[3].reference.text, "Model");
    }

    #[test]
    fn lowers_a_pure_dotted_subject_binding_as_a_feature_chain() {
        let project = Project::from_sources(vec![(
            PathBuf::from("binding.sysml"),
            r#"
package Model {
    part def Computer;
    part def Context {
        part computer : Computer;
    }
    part context : Context;
    verification check {
        subject tested = context.computer;
    }
}
"#
            .to_owned(),
        )])
        .expect("project should lower");

        let subject = project
            .elements
            .iter()
            .find(|element| element.name.as_deref() == Some("tested"))
            .expect("verification subject should exist");
        let binding = subject
            .referenced_feature
            .as_ref()
            .expect("pure dotted expression should lower as a reference");
        assert_eq!(binding.text, "context.computer");
        assert_eq!(binding.segments, ["context", "computer"]);
        assert_eq!(binding.form, ReferenceForm::FeatureChain);
    }

    #[test]
    fn keeps_members_below_an_unmodeled_perform_owner_out_of_the_verification() {
        let project = Project::from_sources(vec![(
            PathBuf::from("ownership.sysml"),
            r#"
package Model {
    requirement selected;
    verification def Check {
        objective direct {
            verify selected;
        }
        perform action {
            objective nested {
                verify selected;
            }
        }
    }
}
"#
            .to_owned(),
        )])
        .expect("project should lower");

        let check = project
            .elements
            .iter()
            .find(|element| element.name.as_deref() == Some("Check"))
            .expect("verification should exist");
        let verification = project
            .verifications
            .iter()
            .find(|verification| verification.element == check.id)
            .expect("verification record should exist");
        assert_eq!(verification.objectives.len(), 1);
        assert_eq!(
            project.element(verification.objectives[0]).name.as_deref(),
            Some("direct")
        );

        let nested = project
            .elements
            .iter()
            .find(|element| element.name.as_deref() == Some("nested"))
            .expect("nested objective should still be lowered");
        assert_ne!(nested.owner, Some(check.id));
        assert_eq!(
            nested.owner.map(|owner| project.element(owner).kind),
            Some(ElementKind::GenericUsage)
        );
    }

    #[test]
    fn keeps_members_below_loop_and_body_wrapped_statement_owners_separate() {
        let project = Project::from_sources(vec![(
            PathBuf::from("control-ownership.sysml"),
            r#"
package Model {
    requirement selected;
    verification def Check {
        doc /* Verification text. */
        part start;
        part finish;
        objective direct {
            verify selected;
        }
        for item in items {
            doc /* Loop text. */
            objective belowLoop {
                verify selected;
            }
        }
        first start then finish {
            objective belowFirst {
                verify selected;
            }
        }
    }
}
"#
            .to_owned(),
        )])
        .expect("project should lower");

        let check = project
            .elements
            .iter()
            .find(|element| element.name.as_deref() == Some("Check"))
            .expect("verification should exist");
        let verification = project
            .verifications
            .iter()
            .find(|verification| verification.element == check.id)
            .expect("verification record should exist");
        assert_eq!(verification.objectives.len(), 1);
        assert_eq!(check.documentation.len(), 1);
        assert_eq!(check.documentation[0].text, "Verification text.");

        for name in ["belowLoop", "belowFirst"] {
            let nested = project
                .elements
                .iter()
                .find(|element| element.name.as_deref() == Some(name))
                .expect("nested objective should still be lowered");
            let owner = nested.owner.expect("nested objective should have an owner");
            assert_eq!(project.element(owner).kind, ElementKind::GenericUsage);
            assert_ne!(owner, check.id);
        }

        let loop_objective = project
            .elements
            .iter()
            .find(|element| element.name.as_deref() == Some("belowLoop"))
            .expect("loop objective should exist");
        let loop_owner = project.element(loop_objective.owner.expect("loop should own objective"));
        assert_eq!(loop_owner.documentation.len(), 1);
        assert_eq!(loop_owner.documentation[0].text, "Loop text.");
    }

    #[test]
    fn retains_recognized_kinds_for_body_bearing_definitions() {
        let project = Project::from_sources(vec![(
            PathBuf::from("body-kinds.sysml"),
            r#"
package Model {
    part def System {
        part child;
    }
    part system : System {
        part component;
    }
    constraint def Limit {
        true
    }
    constraint active : Limit {
        true
    }
}
"#
            .to_owned(),
        )])
        .expect("project should lower");

        let system = project
            .elements
            .iter()
            .find(|element| element.name.as_deref() == Some("System"))
            .expect("part definition should exist");
        let limit = project
            .elements
            .iter()
            .find(|element| element.name.as_deref() == Some("Limit"))
            .expect("constraint definition should exist");
        let child = project
            .elements
            .iter()
            .find(|element| element.name.as_deref() == Some("child"))
            .expect("nested part should exist");
        let system_usage = project
            .elements
            .iter()
            .find(|element| element.name.as_deref() == Some("system"))
            .expect("part usage should exist");
        let active = project
            .elements
            .iter()
            .find(|element| element.name.as_deref() == Some("active"))
            .expect("constraint usage should exist");

        assert_eq!(system.kind, ElementKind::PartDefinition);
        assert_eq!(limit.kind, ElementKind::ConstraintDefinition);
        assert_eq!(system_usage.kind, ElementKind::PartUsage);
        assert_eq!(active.kind, ElementKind::ConstraintUsage);
        assert_eq!(child.owner, Some(system.id));
    }

    #[test]
    fn traverses_inline_constraint_bodies_without_requirement_context_leakage() {
        let project = Project::from_sources(vec![(
            PathBuf::from("constraint-bodies.sysml"),
            r#"
package Model {
    part def Computer;
    requirement def Outer {
        subject outer : Computer;
        assume constraint {
            private import Missing::Thing;
            requirement def Inner {
                subject inner : Computer;
            }
        }
        require constraint {
            private part hidden;
        }
    }
}
"#
            .to_owned(),
        )])
        .expect("project should lower");

        let outer = project
            .requirements
            .iter()
            .find(|requirement| {
                project.element(requirement.element).name.as_deref() == Some("Outer")
            })
            .expect("outer requirement should exist");
        assert_eq!(outer.subjects.len(), 1);
        assert_eq!(outer.assumptions.len(), 1);
        assert_eq!(outer.required_constraints.len(), 1);
        assert!(outer.nested_requirements.is_empty());

        let assumption = outer.assumptions[0];
        let required = outer.required_constraints[0];
        let inner = project
            .requirements
            .iter()
            .find(|requirement| {
                project.element(requirement.element).name.as_deref() == Some("Inner")
            })
            .expect("nested requirement should still be lowered");
        assert_eq!(project.element(inner.element).owner, Some(assumption));
        assert_eq!(inner.subjects.len(), 1);

        let import = project
            .imports
            .first()
            .expect("nested import should be lowered");
        assert_eq!(import.scope, Some(assumption));
        assert_eq!(import.reference.text, "Missing::Thing");
        let hidden = project
            .elements
            .iter()
            .find(|element| element.name.as_deref() == Some("hidden"))
            .expect("private nested part should be lowered");
        assert_eq!(hidden.owner, Some(required));
        assert_eq!(project.profile_issues.len(), 1);
        assert_eq!(
            project.profile_issues[0].code,
            "semantic.profile.unsupported_visibility"
        );
    }

    #[test]
    fn keeps_nested_requirement_and_verification_contexts_separate() {
        let project = Project::from_sources(vec![(
            PathBuf::from("contexts.sysml"),
            r#"
package Model {
    part def Thing;
    requirement def OuterRequirement {
        verification def NestedVerification {
            subject verified : Thing;
        }
        subject required : Thing;
    }
    verification def OuterVerification {
        requirement def NestedRequirement {
            subject required : Thing;
        }
        subject verified : Thing;
    }
}
"#
            .to_owned(),
        )])
        .expect("project should lower");

        for name in ["OuterRequirement", "NestedRequirement"] {
            let element = project
                .elements
                .iter()
                .find(|element| element.name.as_deref() == Some(name))
                .expect("requirement should exist");
            let requirement = project
                .requirements
                .iter()
                .find(|requirement| requirement.element == element.id)
                .expect("requirement record should exist");
            assert_eq!(requirement.subjects.len(), 1, "{name}");
        }
        for name in ["NestedVerification", "OuterVerification"] {
            let element = project
                .elements
                .iter()
                .find(|element| element.name.as_deref() == Some(name))
                .expect("verification should exist");
            let verification = project
                .verifications
                .iter()
                .find(|verification| verification.element == element.id)
                .expect("verification record should exist");
            assert_eq!(verification.subjects.len(), 1, "{name}");
        }
    }

    #[test]
    fn lowers_each_usage_specialization_and_keeps_verify_body_docs_separate() {
        let project = Project::from_sources(vec![(
            PathBuf::from("relationships.sysml"),
            r#"
package Model {
    requirement first;
    requirement second;
    requirement selected :> first :> second;
    verification def Check {
        objective checkObjective {
            doc /* Objective text. */
            verify selected {
                doc /* Relationship text. */
            }
        }
    }
}
"#
            .to_owned(),
        )])
        .expect("project should lower");

        let selected = project
            .elements
            .iter()
            .find(|element| element.name.as_deref() == Some("selected"))
            .expect("specialized requirement usage should exist");
        assert_eq!(
            selected
                .specializations
                .iter()
                .map(|reference| reference.text.as_str())
                .collect::<Vec<_>>(),
            vec!["first", "second"]
        );

        let objective = project
            .elements
            .iter()
            .find(|element| element.name.as_deref() == Some("checkObjective"))
            .expect("objective should exist");
        assert_eq!(objective.documentation.len(), 1);
        assert_eq!(objective.documentation[0].text, "Objective text.");
    }

    #[test]
    fn captures_each_inline_verified_requirement_specialization() {
        let project = Project::from_sources(vec![(
            PathBuf::from("inline-verification.sysml"),
            r#"
package Model {
    requirement first;
    requirement second;
    verification def Check {
        objective {
            verify requirement child :> first :> second;
        }
    }
}
"#
            .to_owned(),
        )])
        .expect("project should lower");

        let assertion = project
            .verify_assertions
            .first()
            .expect("inline verification should be lowered");
        assert!(assertion.target.is_none());
        let inline = assertion
            .inline_requirement
            .expect("assertion should point to its inline requirement usage");
        let inline_element = project.element(inline);
        assert_eq!(inline_element.kind, ElementKind::RequirementUsage);
        assert_eq!(inline_element.name.as_deref(), Some("child"));
        assert_eq!(inline_element.owner, Some(assertion.owner));
        assert_eq!(
            inline_element
                .specializations
                .iter()
                .map(|reference| reference.text.as_str())
                .collect::<Vec<_>>(),
            ["first", "second"]
        );
        assert_eq!(
            project
                .requirements
                .iter()
                .filter(|requirement| requirement.element == inline)
                .count(),
            1
        );
        let objective = project
            .requirements
            .iter()
            .find(|requirement| requirement.element == assertion.owner)
            .expect("objective should have a requirement record");
        assert_eq!(objective.nested_requirements, [inline]);
    }

    #[test]
    fn lowers_objective_members_as_a_composite_requirement_usage() {
        let project = Project::from_sources(vec![(
            PathBuf::from("objective.sysml"),
            r#"
package Model {
    part def Computer;
    constraint rule;
    requirement def ObjectiveRequirement {
        subject expected : Computer;
    }
    verification def Check {
        objective selected : ObjectiveRequirement {
            subject actual : Computer;
            actor operator : Computer;
            assume rule;
            require rule;
            requirement nested;
        }
    }
}
"#
            .to_owned(),
        )])
        .expect("project should lower");

        let check = project
            .elements
            .iter()
            .find(|element| element.name.as_deref() == Some("Check"))
            .expect("verification should exist");
        let selected = project
            .elements
            .iter()
            .find(|element| element.name.as_deref() == Some("selected"))
            .expect("objective should exist");
        assert_eq!(selected.kind, ElementKind::Objective);
        assert_eq!(selected.owner, Some(check.id));

        let objective = project
            .requirements
            .iter()
            .find(|requirement| requirement.element == selected.id)
            .expect("objective should have a requirement record");
        assert_eq!(objective.subjects.len(), 1);
        assert_eq!(objective.actors.len(), 1);
        assert!(objective.stakeholders.is_empty());
        assert_eq!(objective.assumptions.len(), 1);
        assert_eq!(objective.required_constraints.len(), 1);
        assert_eq!(objective.nested_requirements.len(), 1);

        let verification = project
            .verifications
            .iter()
            .find(|verification| verification.element == check.id)
            .expect("verification record should exist");
        assert_eq!(verification.objectives, [selected.id]);
    }

    #[test]
    fn preserves_a_direct_specialization_on_a_keywordless_feature_usage() {
        let project = Project::from_sources(vec![(
            PathBuf::from("specialization.sysml"),
            r#"
package Model {
    part base;
    candidate :> base;
    bracedCandidate :> base {}
}
"#
            .to_owned(),
        )])
        .expect("project should lower");

        let candidate = project
            .elements
            .iter()
            .find(|element| element.name.as_deref() == Some("candidate"))
            .expect("keywordless feature usage should exist");
        assert_eq!(candidate.kind, ElementKind::GenericUsage);
        assert_eq!(
            candidate
                .specializations
                .iter()
                .map(|reference| reference.text.as_str())
                .collect::<Vec<_>>(),
            ["base"]
        );
        let braced = project
            .elements
            .iter()
            .find(|element| element.name.as_deref() == Some("bracedCandidate"))
            .expect("braced keywordless feature usage should exist");
        assert_eq!(braced.kind, ElementKind::GenericUsage);
        assert_eq!(
            braced
                .specializations
                .iter()
                .map(|reference| reference.text.as_str())
                .collect::<Vec<_>>(),
            ["base"]
        );
    }
}
