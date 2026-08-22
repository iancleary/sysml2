use crate::check::{check_sources, load_model_sources};
use crate::project::{
    Element, ElementId, ElementKind, Import, ImportVisibility, Project, Reference, ReferenceForm,
    Requirement, Satisfaction, SourceId, Span, VerifyAssertion,
};
use crate::{CheckDiagnostic, CheckError, CheckFileReport, CheckSpan};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

/// Version of the machine-readable `sysml validate` report.
pub const VALIDATION_REPORT_SCHEMA_VERSION: u32 = 1;

/// The bounded SysML 2.0 requirements structure profile implemented here.
pub const REQUIREMENTS_STRUCTURE_PROFILE_ID: &str = "sysml-2.0-requirements-structure-v1";

const REQUIREMENTS_STRUCTURE_SOURCE_COMMIT: &str = "9baca5908ca28b53da085de69336fde48420ea8f";

/// Typed selector for the semantic profile applied by [`validate_paths`].
///
/// Callers must choose a profile explicitly. New profile variants may be added
/// in later releases without making downstream matches exhaustive.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationProfileId {
    RequirementsStructureV1,
}

impl ValidationProfileId {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RequirementsStructureV1 => REQUIREMENTS_STRUCTURE_PROFILE_ID,
        }
    }

    fn metadata(self) -> ValidationProfile {
        match self {
            Self::RequirementsStructureV1 => ValidationProfile::requirements_structure(),
        }
    }
}

/// Standards identity attached to every validation report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ValidationProfile {
    pub id: String,
    pub language: String,
    pub language_version: String,
    pub source_release: String,
    pub source_commit: String,
    pub metamodel_version: String,
}

impl ValidationProfile {
    fn requirements_structure() -> Self {
        Self {
            id: REQUIREMENTS_STRUCTURE_PROFILE_ID.to_owned(),
            language: "SysML".to_owned(),
            language_version: "2.0".to_owned(),
            source_release: "2026-04".to_owned(),
            source_commit: REQUIREMENTS_STRUCTURE_SOURCE_COMMIT.to_owned(),
            metamodel_version: "20250201".to_owned(),
        }
    }
}

/// Deterministic syntax and semantic result for an explicit validation profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ValidationReport {
    pub schema_version: u32,
    pub tool_version: String,
    pub profile: ValidationProfile,
    pub valid: bool,
    pub files: Vec<CheckFileReport>,
}

/// Validate `.sysml` inputs against the bounded requirements structure profile.
///
/// The syntax pass runs first. Semantic processing is skipped when any syntax
/// diagnostic exists so that malformed trees cannot produce cascaded findings.
pub fn validate_paths(
    paths: &[PathBuf],
    profile: ValidationProfileId,
) -> Result<ValidationReport, CheckError> {
    let sources = load_model_sources(paths)?;
    let syntax = check_sources(&sources)?;
    if !syntax.valid {
        return Ok(ValidationReport {
            schema_version: VALIDATION_REPORT_SCHEMA_VERSION,
            tool_version: env!("CARGO_PKG_VERSION").to_owned(),
            profile: profile.metadata(),
            valid: false,
            files: syntax.files,
        });
    }

    let project = Project::from_sources(sources)?;
    let semantic_diagnostics = match profile {
        ValidationProfileId::RequirementsStructureV1 => {
            RequirementsValidator::new(&project).validate()
        }
    };
    let mut files = syntax.files;
    merge_diagnostics(&project, &mut files, semantic_diagnostics);
    let valid = files.iter().all(|file| file.valid);

    Ok(ValidationReport {
        schema_version: VALIDATION_REPORT_SCHEMA_VERSION,
        tool_version: env!("CARGO_PKG_VERSION").to_owned(),
        profile: profile.metadata(),
        valid,
        files,
    })
}

#[derive(Debug)]
struct SemanticDiagnostic {
    span: Span,
    code: &'static str,
    message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Resolution {
    Found(ElementId),
    Missing,
    Ambiguous,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolutionFailure {
    reference: Reference,
    resolution: Resolution,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolutionOutcome {
    resolution: Resolution,
    known_ids: BTreeSet<ElementId>,
    definite_ids: BTreeSet<ElementId>,
    failures: Vec<ResolutionFailure>,
    main_is_proven: bool,
}

impl ResolutionOutcome {
    fn from_candidates(candidates: impl IntoIterator<Item = ElementId>) -> Self {
        let known_ids = candidates.into_iter().collect::<BTreeSet<_>>();
        let resolution = match known_ids.len() {
            0 => Resolution::Missing,
            1 => Resolution::Found(
                *known_ids
                    .first()
                    .expect("one candidate should have a first element"),
            ),
            _ => Resolution::Ambiguous,
        };
        let main_is_proven = known_ids.len() > 1;
        Self {
            resolution,
            definite_ids: known_ids.clone(),
            known_ids,
            failures: Vec::new(),
            main_is_proven,
        }
    }

    fn missing() -> Self {
        Self::from_candidates([])
    }

    fn extend_failures(&mut self, failures: Vec<ResolutionFailure>) {
        for failure in failures {
            if self.failures.iter().any(|existing| {
                existing.reference.span == failure.reference.span
                    && existing.resolution == failure.resolution
            }) {
                continue;
            }
            self.failures.push(failure);
        }
    }

    fn extend_higher_precedence_failures(&mut self, failures: Vec<ResolutionFailure>) {
        let precedence_tainted = !failures.is_empty();
        self.extend_failures(failures);
        if precedence_tainted {
            self.definite_ids.clear();
            self.main_is_proven = false;
        }
    }

    fn advance(self, mut next: Self) -> Self {
        let prefix_was_tainted = !self.failures.is_empty();
        let next_was_clean = next.failures.is_empty();
        let next_was_independently_proven = next.main_is_proven;
        next.extend_failures(self.failures);
        if prefix_was_tainted {
            next.definite_ids.clear();
            if !matches!(next.resolution, Resolution::Found(_))
                && (next_was_clean || next_was_independently_proven)
            {
                next.main_is_proven = true;
            }
        }
        next
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Conformance {
    Conforms,
    DoesNotConform,
    Indeterminate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SubjectTypeOutcome {
    Resolved(ElementId),
    Missing,
    Invalid,
}

#[derive(Default)]
struct ResolutionCandidates {
    known_ids: BTreeSet<ElementId>,
    definite_ids: BTreeSet<ElementId>,
    ambiguous: bool,
    proven_ambiguous: bool,
    proven_missing: bool,
    failures: Vec<ResolutionFailure>,
}

impl ResolutionCandidates {
    fn include(&mut self, outcome: ResolutionOutcome) {
        let ResolutionOutcome {
            resolution,
            known_ids,
            definite_ids,
            failures,
            main_is_proven,
        } = outcome;
        let clean = failures.is_empty();
        self.known_ids.extend(known_ids);
        if clean {
            self.definite_ids.extend(definite_ids);
        }
        self.extend_failures(failures);
        match resolution {
            Resolution::Found(id) => {
                self.known_ids.insert(id);
                if clean {
                    self.definite_ids.insert(id);
                }
            }
            Resolution::Ambiguous => {
                self.ambiguous = true;
                self.proven_ambiguous |= main_is_proven;
            }
            Resolution::Missing => self.proven_missing |= main_is_proven,
        }
    }

    fn extend_failures(&mut self, failures: Vec<ResolutionFailure>) {
        for failure in failures {
            if self.failures.iter().any(|existing| {
                existing.reference.span == failure.reference.span
                    && existing.resolution == failure.resolution
            }) {
                continue;
            }
            self.failures.push(failure);
        }
    }

    fn finish(self) -> ResolutionOutcome {
        let distinct_known_ids = self.known_ids.len();
        let resolution = if self.ambiguous || distinct_known_ids > 1 {
            Resolution::Ambiguous
        } else if let Some(id) = self.known_ids.first() {
            Resolution::Found(*id)
        } else {
            Resolution::Missing
        };
        let main_is_proven = match resolution {
            Resolution::Ambiguous => self.definite_ids.len() > 1 || self.proven_ambiguous,
            Resolution::Missing => self.proven_missing,
            Resolution::Found(_) => false,
        };
        ResolutionOutcome {
            resolution,
            known_ids: self.known_ids,
            definite_ids: self.definite_ids,
            failures: self.failures,
            main_is_proven,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum MemberAccess {
    Local,
    Qualified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImportLookupScope {
    Root(SourceId),
    Owned(ElementId),
}

impl ImportLookupScope {
    fn contains(self, import: &Import) -> bool {
        match self {
            Self::Root(source) => import.scope.is_none() && import.span.source == source,
            Self::Owned(owner) => import.scope == Some(owner),
        }
    }
}

#[derive(Default)]
struct ResolutionState {
    imports: BTreeSet<(usize, usize)>,
    members: BTreeSet<(ElementId, String, MemberAccess)>,
}

struct RequirementsValidator<'a> {
    project: &'a Project,
    diagnostics: Vec<SemanticDiagnostic>,
}

impl<'a> RequirementsValidator<'a> {
    fn new(project: &'a Project) -> Self {
        Self {
            project,
            diagnostics: Vec::new(),
        }
    }

    fn validate(mut self) -> Vec<SemanticDiagnostic> {
        for issue in self.project.profile_issues.clone() {
            self.push(issue.span, issue.code, issue.message);
        }
        self.validate_imports();
        self.validate_requirement_structure();
        self.validate_satisfactions();
        self.validate_verifications();
        self.diagnostics.sort_by(|left, right| {
            let left_source = &self.project.source(left.span.source).path;
            let right_source = &self.project.source(right.span.source).path;
            left_source
                .cmp(right_source)
                .then_with(|| left.span.start.byte.cmp(&right.span.start.byte))
                .then_with(|| left.code.cmp(right.code))
                .then_with(|| left.message.cmp(&right.message))
        });
        self.diagnostics
    }

    fn validate_imports(&mut self) {
        let imports = self.project.imports.clone();
        for import in &imports {
            if import.visibility == ImportVisibility::Missing {
                self.push(
                    import.span,
                    "semantic.import.visibility",
                    "an import requires an explicit public, private, or protected visibility indicator",
                );
                continue;
            }
            if import.visibility == ImportVisibility::Protected {
                self.push(
                    import.span,
                    "semantic.profile.unsupported_import",
                    "protected imports are outside this validation profile",
                );
                continue;
            }
            if import.scope.is_none() && import.visibility != ImportVisibility::Private {
                self.push(
                    import.span,
                    "semantic.import.visibility",
                    "a top-level import must have private visibility",
                );
                continue;
            }
            if import.all {
                self.push(
                    import.span,
                    "semantic.profile.unsupported_import",
                    "imports with the `all` modifier are outside this validation profile",
                );
                continue;
            }
            if import.recursive || import.filtered {
                self.push(
                    import.span,
                    "semantic.profile.unsupported_import",
                    "recursive and filtered wildcard imports are outside this validation profile",
                );
                continue;
            }
            self.validate_import_target_reference(import);
        }
    }

    fn validate_requirement_structure(&mut self) {
        let requirements = self.project.requirements.clone();
        for requirement in &requirements {
            let element = self.project.element(requirement.element);
            self.validate_requirement_types(element);
            self.validate_subjects(requirement);
            self.validate_requirement_members(requirement);
        }
    }

    fn validate_requirement_types(&mut self, element: &Element) {
        if element.declared_types.len() > 1 {
            self.push(
                element.declared_types[1].span,
                "semantic.requirement.type_cardinality",
                "a requirement usage may have at most one explicit requirement definition type",
            );
        }
        self.validate_parent_relationship_cardinality(element, "requirement");
        for reference in &element.declared_types {
            if let Some(target) = self.resolve_for_element(reference, element) {
                if self.project.element(target).kind != ElementKind::RequirementDefinition {
                    self.push(
                        reference.span,
                        "semantic.requirement.type_kind",
                        format!(
                            "requirement type {:?} resolves to {}, not a requirement definition",
                            reference.text,
                            kind_name(self.project.element(target).kind)
                        ),
                    );
                }
            }
        }

        let usage_specialization = element.kind.is_requirement_usage();
        for reference in &element.specializations {
            if let Some(target) = self.resolve_for_element(reference, element) {
                let target_kind = self.project.element(target).kind;
                let valid_kind = if usage_specialization {
                    target_kind.is_requirement_usage()
                } else {
                    target_kind == ElementKind::RequirementDefinition
                };
                if !valid_kind {
                    self.push(
                        reference.span,
                        "semantic.requirement.specialization_kind",
                        format!(
                            "requirement specialization {:?} resolves to {}, not {}",
                            reference.text,
                            kind_name(target_kind),
                            if usage_specialization {
                                "a requirement usage"
                            } else {
                                "a requirement definition"
                            }
                        ),
                    );
                }
            }
        }
        if self.requirement_parent_chain_is_cyclic(element.id) {
            let reference = element
                .declared_types
                .first()
                .or_else(|| element.specializations.first())
                .expect("a cyclic requirement parent chain has a parent reference");
            self.push(
                reference.span,
                "semantic.requirement.inheritance_cycle",
                "requirement type and specialization parent chains must be acyclic",
            );
        }
    }

    fn validate_subjects(&mut self, requirement: &Requirement) {
        if requirement.subjects.len() > 1 {
            let duplicate = self.project.element(requirement.subjects[1]);
            self.push(
                duplicate.span,
                "semantic.requirement.subject_cardinality",
                "a requirement may own at most one explicit subject",
            );
        }
        for subject in &requirement.subjects {
            let subject = self.project.element(*subject).clone();
            self.validate_effective_type_chain(&subject, "requirement subject");
            self.validate_binding_target_types(
                &subject,
                "requirement subject",
                "semantic.requirement.subject_binding",
            );
        }
        for actor in &requirement.actors {
            self.validate_part_parameter(*actor, "actor");
        }
        for stakeholder in &requirement.stakeholders {
            self.validate_part_parameter(*stakeholder, "stakeholder");
        }
        self.validate_requirement_subject_binding(requirement);
    }

    fn validate_requirement_subject_binding(&mut self, requirement: &Requirement) {
        let element = self.project.element(requirement.element);
        let Some(subject_id) = requirement.subjects.first() else {
            return;
        };
        let subject = self.project.element(*subject_id);
        if has_multiple_parent_relationships(subject) {
            return;
        }
        let declared_type = self.first_resolved_type(subject);
        if !subject.declared_types.is_empty() && declared_type.is_none() {
            return;
        }
        let inherited_type = element
            .kind
            .is_requirement_usage()
            .then(|| self.inherited_requirement_subject_type(requirement.element))
            .flatten();
        if let (Some(declared), Some(inherited)) = (declared_type, inherited_type) {
            match self.is_same_or_specializes(declared, inherited, &mut BTreeSet::new()) {
                Conformance::Conforms => {}
                Conformance::DoesNotConform => {
                    self.push(
                        subject.span,
                        "semantic.requirement.subject_binding",
                        "requirement subject type does not conform to the definition subject type",
                    );
                    return;
                }
                Conformance::Indeterminate => return,
            }
        }
        let Some(expected_type) = declared_type.or(inherited_type) else {
            return;
        };
        if subject.referenced_feature.as_ref().is_some_and(|binding| {
            !matches!(
                self.resolve_reference(binding, subject.owner, subject.package),
                Resolution::Found(_)
            )
        }) {
            return;
        }
        if self.effective_type_chain_is_invalid(subject.id)
            || self.binding_target_type_chain_is_invalid(subject)
        {
            return;
        }
        let actual_type = self.resolved_subject_type(subject);
        if let Some(actual) = actual_type {
            match self.is_same_or_specializes(actual, expected_type, &mut BTreeSet::new()) {
                Conformance::Conforms | Conformance::Indeterminate => return,
                Conformance::DoesNotConform => {}
            }
        }
        let span = subject
            .referenced_feature
            .as_ref()
            .map_or(subject.span, |binding| binding.span);
        self.push(
            span,
            "semantic.requirement.subject_binding",
            "requirement subject binding does not conform to the definition subject type",
        );
    }

    fn validate_part_parameter(&mut self, id: ElementId, role: &str) {
        let parameter = self.project.element(id);
        for reference in &parameter.declared_types {
            if let Some(target) = self.resolve_for_element(reference, parameter) {
                if self.project.element(target).kind != ElementKind::PartDefinition {
                    self.push(
                        reference.span,
                        "semantic.requirement.part_parameter_type",
                        format!(
                            "requirement {role} type {:?} does not resolve to a part definition",
                            reference.text
                        ),
                    );
                }
            }
        }
    }

    fn validate_requirement_members(&mut self, requirement: &Requirement) {
        for member in requirement
            .assumptions
            .iter()
            .chain(requirement.required_constraints.iter())
        {
            let member = self.project.element(*member);
            self.resolve_declared_types(member, Some(ElementKind::ConstraintDefinition));
            let Some(reference) = &member.referenced_feature else {
                continue;
            };
            if let Some(target) = self.resolve_for_element(reference, member) {
                let target_kind = self.project.element(target).kind;
                if target_kind != ElementKind::ConstraintUsage
                    && !target_kind.is_requirement_usage()
                {
                    self.push(
                        reference.span,
                        "semantic.requirement.required_member_kind",
                        format!(
                            "requirement member {:?} resolves to {}, not a constraint or requirement usage",
                            reference.text,
                            kind_name(target_kind)
                        ),
                    );
                }
            }
        }
    }

    fn validate_satisfactions(&mut self) {
        let satisfactions = self.project.satisfactions.clone();
        for satisfaction in &satisfactions {
            let requirement_id = if let Some(inline) = satisfaction.inline_requirement {
                inline
            } else {
                let Some(requirement) = &satisfaction.requirement else {
                    continue;
                };
                let Some(id) = self.resolve_profile_reference(
                    requirement,
                    satisfaction.owner,
                    satisfaction.package,
                    "requirement satisfaction target",
                ) else {
                    continue;
                };
                id
            };
            if !self
                .project
                .element(requirement_id)
                .kind
                .is_requirement_usage()
            {
                let requirement = satisfaction
                    .requirement
                    .as_ref()
                    .expect("non-usage satisfaction targets are references");
                self.push(
                    requirement.span,
                    "semantic.satisfaction.target_kind",
                    format!(
                        "satisfaction target {:?} resolves to {}, not a requirement usage",
                        requirement.text,
                        kind_name(self.project.element(requirement_id).kind)
                    ),
                );
                continue;
            }

            if let Some(subject_reference) = &satisfaction.subject {
                if let Some(subject_id) = self.resolve_profile_reference(
                    subject_reference,
                    satisfaction.owner,
                    satisfaction.package,
                    "satisfying subject",
                ) {
                    self.validate_satisfaction_subject(
                        satisfaction,
                        requirement_id,
                        subject_id,
                        subject_reference,
                    );
                }
            }
        }
    }

    fn validate_satisfaction_subject(
        &mut self,
        _satisfaction: &Satisfaction,
        requirement_id: ElementId,
        subject_id: ElementId,
        subject_reference: &Reference,
    ) {
        let subject = self.project.element(subject_id).clone();
        if !subject.kind.is_usage() || subject.kind.is_requirement_usage() {
            self.push(
                subject_reference.span,
                "semantic.satisfaction.subject_kind",
                format!(
                    "satisfying subject {:?} must resolve to a non-requirement usage",
                    subject_reference.text
                ),
            );
            return;
        }

        let Some(expected_type) = self.effective_requirement_subject_type(requirement_id) else {
            return;
        };
        self.validate_effective_type_chain(&subject, "satisfying subject");
        self.validate_binding_target_types(
            &subject,
            "satisfying subject",
            "semantic.satisfaction.subject_kind",
        );
        if self.effective_type_chain_is_invalid(subject.id)
            || self.binding_target_type_chain_is_invalid(&subject)
        {
            return;
        }
        let Some(actual_type) = self.resolved_subject_type(&subject) else {
            self.push(
                subject_reference.span,
                "semantic.satisfaction.subject_conformance",
                format!(
                    "satisfying subject {:?} has no resolvable declared type",
                    subject_reference.text
                ),
            );
            return;
        };
        match self.is_same_or_specializes(actual_type, expected_type, &mut BTreeSet::new()) {
            Conformance::Conforms | Conformance::Indeterminate => {}
            Conformance::DoesNotConform => {
                self.push(
                    subject_reference.span,
                    "semantic.satisfaction.subject_conformance",
                    format!(
                        "satisfying subject {:?} is not typed by the requirement subject type",
                        subject_reference.text
                    ),
                );
            }
        }
    }

    fn validate_verifications(&mut self) {
        let verifications = self.project.verifications.clone();
        for verification in &verifications {
            self.validate_verification_types(self.project.element(verification.element));
            if verification.objectives.len() > 1 {
                let duplicate = self.project.element(verification.objectives[1]);
                self.push(
                    duplicate.span,
                    "semantic.verification.objective_cardinality",
                    "a verification case may own at most one objective",
                );
            }
            for subject in &verification.subjects {
                let subject = self.project.element(*subject).clone();
                self.validate_effective_type_chain(&subject, "verification subject");
                self.validate_binding_target_types(
                    &subject,
                    "verification subject",
                    "semantic.verification.subject_binding",
                );
            }
            self.validate_verification_subject_binding(verification);
        }

        let assertions = self.project.verify_assertions.clone();
        for assertion in &assertions {
            self.validate_verify_assertion(assertion);
        }
    }

    fn validate_verification_types(&mut self, element: &Element) {
        if element.declared_types.len() > 1 {
            self.push(
                element.declared_types[1].span,
                "semantic.verification.case_type_cardinality",
                "a verification usage may have at most one explicit verification definition type",
            );
        }
        self.validate_parent_relationship_cardinality(element, "verification");
        for reference in &element.declared_types {
            if let Some(target) = self.resolve_for_element(reference, element) {
                if self.project.element(target).kind != ElementKind::VerificationDefinition {
                    self.push(
                        reference.span,
                        "semantic.verification.case_type_kind",
                        format!(
                            "verification type {:?} resolves to {}, not a verification definition",
                            reference.text,
                            kind_name(self.project.element(target).kind)
                        ),
                    );
                }
            }
        }
        let specialization_kind = if element.kind == ElementKind::VerificationUsage {
            ElementKind::VerificationUsage
        } else {
            ElementKind::VerificationDefinition
        };
        for reference in &element.specializations {
            if let Some(target) = self.resolve_for_element(reference, element) {
                if self.project.element(target).kind != specialization_kind {
                    self.push(
                        reference.span,
                        "semantic.verification.case_specialization_kind",
                        format!(
                            "verification specialization {:?} resolves to {}, not {}",
                            reference.text,
                            kind_name(self.project.element(target).kind),
                            kind_name(specialization_kind)
                        ),
                    );
                }
            }
        }
        if self.verification_parent_chain_is_cyclic(element.id) {
            let reference = element
                .declared_types
                .first()
                .or_else(|| element.specializations.first())
                .expect("a cyclic verification parent chain has a parent reference");
            self.push(
                reference.span,
                "semantic.verification.inheritance_cycle",
                "verification type and specialization parent chains must be acyclic",
            );
        }
    }

    fn validate_verification_subject_binding(
        &mut self,
        verification: &crate::project::Verification,
    ) {
        let Some(subject_id) = verification.subjects.first() else {
            return;
        };
        let subject = self.project.element(*subject_id);
        if has_multiple_parent_relationships(subject) {
            return;
        }
        let declared_type = self.first_resolved_type(subject);
        if !subject.declared_types.is_empty() && declared_type.is_none() {
            return;
        }
        let inherited_type = self.inherited_verification_subject_type(verification.element);
        if let (Some(declared), Some(inherited)) = (declared_type, inherited_type) {
            match self.is_same_or_specializes(declared, inherited, &mut BTreeSet::new()) {
                Conformance::Conforms => {}
                Conformance::DoesNotConform => {
                    self.push(
                        subject.span,
                        "semantic.verification.subject_binding",
                        "verification subject type does not conform to the definition subject type",
                    );
                    return;
                }
                Conformance::Indeterminate => return,
            }
        }
        let Some(expected_type) = declared_type.or(inherited_type) else {
            return;
        };
        if subject.referenced_feature.as_ref().is_some_and(|binding| {
            !matches!(
                self.resolve_reference(binding, subject.owner, subject.package),
                Resolution::Found(_)
            )
        }) {
            return;
        }
        if self.effective_type_chain_is_invalid(subject.id)
            || self.binding_target_type_chain_is_invalid(subject)
        {
            return;
        }
        if let Some(actual) = self.resolved_subject_type(subject) {
            match self.is_same_or_specializes(actual, expected_type, &mut BTreeSet::new()) {
                Conformance::Conforms | Conformance::Indeterminate => return,
                Conformance::DoesNotConform => {}
            }
        }
        let span = subject
            .referenced_feature
            .as_ref()
            .map_or(subject.span, |binding| binding.span);
        self.push(
            span,
            "semantic.verification.subject_binding",
            "verification subject binding does not conform to the definition subject type",
        );
    }

    fn validate_verify_assertion(&mut self, assertion: &VerifyAssertion) {
        let verification = self.verification_owner(assertion);
        if verification.is_none() {
            self.push(
                assertion.span,
                "semantic.verification.placement",
                "a requirement verification must be inside a verification-case objective",
            );
        }

        let (requirement, span) = if let Some(inline) = assertion.inline_requirement {
            debug_assert!(self.project.element(inline).kind.is_requirement_usage());
            (Some(inline), assertion.span)
        } else if let Some(reference) = &assertion.target {
            let target = self.resolve_profile_reference(
                reference,
                Some(assertion.owner),
                assertion.package,
                "verified requirement target",
            );
            let target = target.filter(|target| {
                if self.project.element(*target).kind.is_requirement_usage() {
                    true
                } else {
                    self.push(
                        reference.span,
                        "semantic.verification.target_kind",
                        format!(
                            "verification target {:?} resolves to {}, not a requirement usage",
                            reference.text,
                            kind_name(self.project.element(*target).kind)
                        ),
                    );
                    false
                }
            });
            (target, reference.span)
        } else {
            (None, assertion.span)
        };

        if let (Some(verification), Some(requirement)) = (verification, requirement) {
            self.validate_verification_subject_conformance(verification, requirement, span);
        }
    }

    fn verification_owner(&self, assertion: &VerifyAssertion) -> Option<ElementId> {
        let objective = self.project.element(assertion.owner);
        if objective.kind != ElementKind::Objective {
            return None;
        }
        let verification = objective.owner?;
        matches!(
            self.project.element(verification).kind,
            ElementKind::VerificationDefinition | ElementKind::VerificationUsage
        )
        .then_some(verification)
    }

    fn validate_verification_subject_conformance(
        &mut self,
        verification: ElementId,
        requirement: ElementId,
        span: Span,
    ) {
        let Some(expected_type) = self.effective_requirement_subject_type(requirement) else {
            return;
        };
        let actual_type = match self.effective_verification_subject_type_outcome(verification) {
            SubjectTypeOutcome::Resolved(actual_type) => actual_type,
            SubjectTypeOutcome::Invalid => return,
            SubjectTypeOutcome::Missing => {
                self.push(
                    span,
                    "semantic.verification.subject_conformance",
                    "verification case has no resolvable subject type for the verified requirement",
                );
                return;
            }
        };
        match self.is_same_or_specializes(actual_type, expected_type, &mut BTreeSet::new()) {
            Conformance::Conforms | Conformance::Indeterminate => {}
            Conformance::DoesNotConform => {
                self.push(
                    span,
                    "semantic.verification.subject_conformance",
                    "verification case subject does not conform to the verified requirement subject",
                );
            }
        }
    }

    fn resolve_declared_types(&mut self, element: &Element, required_kind: Option<ElementKind>) {
        for reference in &element.declared_types {
            if let Some(target) = self.resolve_for_element(reference, element) {
                if required_kind.is_some_and(|kind| self.project.element(target).kind != kind) {
                    self.push(
                        reference.span,
                        "semantic.requirement.constraint_type_kind",
                        format!(
                            "constraint type {:?} does not resolve to a constraint definition",
                            reference.text
                        ),
                    );
                }
            }
        }
    }

    fn validate_supported_type_cardinality(&mut self, element: &Element, role: &str) {
        if let Some(reference) = unsupported_parent_reference(element) {
            self.push(
                reference.span,
                "semantic.profile.unsupported_multityping",
                format!(
                    "multiple type or specialization parents on a {role} are outside this validation profile"
                ),
            );
        }
    }

    fn validate_effective_type_chain(&mut self, element: &Element, role: &str) {
        let mut current_id = element.id;
        let mut visited = BTreeSet::new();
        while visited.insert(current_id) {
            let current = self.project.element(current_id).clone();
            self.validate_supported_type_cardinality(&current, role);

            for reference in &current.declared_types {
                self.resolve_profile_reference(
                    reference,
                    current.owner,
                    current.package,
                    &format!("{role} type"),
                );
            }

            let mut next_parent = None;
            for (index, reference) in current.specializations.iter().enumerate() {
                let resolved = self.resolve_profile_reference(
                    reference,
                    current.owner,
                    current.package,
                    &format!("{role} specialization"),
                );
                if index == 0 {
                    next_parent = resolved;
                }
            }

            if has_multiple_parent_relationships(&current) || !current.declared_types.is_empty() {
                return;
            }
            let Some(parent) = next_parent else {
                return;
            };
            if !self.project.element(parent).kind.is_usage() {
                return;
            }
            current_id = parent;
        }
    }

    fn validate_parent_relationship_cardinality(&mut self, element: &Element, role: &str) {
        let parent_count = element.declared_types.len() + element.specializations.len();
        if !element.specializations.is_empty() && parent_count > 1 {
            let reference = if element.declared_types.is_empty() {
                &element.specializations[1]
            } else {
                &element.specializations[0]
            };
            self.push(
                reference.span,
                "semantic.profile.unsupported_multityping",
                format!(
                    "multiple type or specialization parents on a {role} are outside this validation profile"
                ),
            );
        }
    }

    fn validate_binding_target_types(
        &mut self,
        element: &Element,
        role: &str,
        target_kind_code: &'static str,
    ) {
        let Some(binding) = &element.referenced_feature else {
            return;
        };
        let Some(target) = self.resolve_profile_reference(
            binding,
            element.owner,
            element.package,
            &format!("{role} binding"),
        ) else {
            return;
        };
        let target = self.project.element(target).clone();
        if !target.kind.is_usage() {
            self.push(
                binding.span,
                target_kind_code,
                format!(
                    "{role} binding {:?} resolves to {}, not a usage",
                    binding.text,
                    kind_name(target.kind)
                ),
            );
            return;
        }
        self.validate_effective_type_chain(&target, &format!("bound {role}"));
    }

    fn binding_target_type_chain_is_invalid(&self, element: &Element) -> bool {
        let Some(binding) = &element.referenced_feature else {
            return false;
        };
        let Resolution::Found(target) =
            self.resolve_reference(binding, element.owner, element.package)
        else {
            return true;
        };
        let target = self.project.element(target);
        !target.kind.is_usage() || self.effective_type_chain_is_invalid(target.id)
    }

    fn effective_type_chain_is_invalid(&self, element_id: ElementId) -> bool {
        self.effective_type_chain_is_invalid_inner(element_id, &mut BTreeSet::new())
    }

    fn effective_type_chain_is_invalid_inner(
        &self,
        element_id: ElementId,
        visited: &mut BTreeSet<ElementId>,
    ) -> bool {
        if !visited.insert(element_id) {
            return false;
        }
        let element = self.project.element(element_id);
        if has_multiple_parent_relationships(element)
            || element
                .declared_types
                .iter()
                .chain(element.specializations.iter())
                .any(|reference| {
                    !matches!(
                        self.resolve_reference(reference, element.owner, element.package),
                        Resolution::Found(_)
                    )
                })
        {
            return true;
        }
        if !element.declared_types.is_empty() {
            return false;
        }
        let Some(specialization) = element.specializations.first() else {
            return false;
        };
        let Resolution::Found(parent) =
            self.resolve_reference(specialization, element.owner, element.package)
        else {
            return true;
        };
        self.project.element(parent).kind.is_usage()
            && self.effective_type_chain_is_invalid_inner(parent, visited)
    }

    fn resolve_for_element(
        &mut self,
        reference: &Reference,
        element: &Element,
    ) -> Option<ElementId> {
        self.resolve_profile_reference(reference, element.owner, element.package, "model reference")
    }

    fn report_resolution(
        &mut self,
        reference: &Reference,
        resolution: Resolution,
        role: &str,
    ) -> Option<ElementId> {
        match resolution {
            Resolution::Found(id) => Some(id),
            Resolution::Missing => {
                self.push_resolution_once(
                    reference.span,
                    "resolution.unresolved_reference",
                    format!("unresolved {role} {:?}", reference.text),
                );
                None
            }
            Resolution::Ambiguous => {
                self.push_resolution_once(
                    reference.span,
                    "resolution.ambiguous_reference",
                    format!("ambiguous {role} {:?}", reference.text),
                );
                None
            }
        }
    }

    fn report_resolution_failures(&mut self, failures: Vec<ResolutionFailure>) {
        for failure in failures {
            let _ = self.report_resolution(
                &failure.reference,
                failure.resolution,
                "traversed parent relationship",
            );
        }
    }

    fn report_resolution_outcome(
        &mut self,
        reference: &Reference,
        outcome: ResolutionOutcome,
        role: &str,
    ) -> Option<ElementId> {
        let ResolutionOutcome {
            resolution,
            known_ids: _,
            definite_ids: _,
            failures,
            main_is_proven,
        } = outcome;
        let tainted = !failures.is_empty();
        self.report_resolution_failures(failures);
        match resolution {
            Resolution::Found(id) if !tainted => Some(id),
            Resolution::Found(_) => None,
            resolution if !tainted || main_is_proven => {
                self.report_resolution(reference, resolution, role)
            }
            Resolution::Missing | Resolution::Ambiguous => None,
        }
    }

    fn parent_failure_outcome(
        reference: &Reference,
        outcome: ResolutionOutcome,
    ) -> ResolutionOutcome {
        let ResolutionOutcome {
            resolution,
            known_ids: _,
            definite_ids: _,
            failures,
            main_is_proven,
        } = outcome;
        let proven_missing = resolution == Resolution::Missing && main_is_proven;
        let report_parent = main_is_proven || failures.is_empty();
        let mut result = ResolutionOutcome::missing();
        result.main_is_proven = proven_missing;
        result.extend_failures(failures);
        if report_parent {
            result.extend_failures(vec![ResolutionFailure {
                reference: reference.clone(),
                resolution,
            }]);
        }
        result
    }

    fn resolve_reference(
        &self,
        reference: &Reference,
        owner: Option<ElementId>,
        package: Option<ElementId>,
    ) -> Resolution {
        let outcome = self.resolve_reference_outcome(reference, owner, package);
        if !outcome.failures.is_empty() && matches!(&outcome.resolution, Resolution::Found(_)) {
            Resolution::Missing
        } else {
            outcome.resolution
        }
    }

    fn resolve_reference_outcome(
        &self,
        reference: &Reference,
        owner: Option<ElementId>,
        package: Option<ElementId>,
    ) -> ResolutionOutcome {
        let mut state = ResolutionState::default();
        self.resolve_reference_guarded(reference, owner, package, &mut state)
    }

    fn resolve_profile_reference(
        &mut self,
        reference: &Reference,
        owner: Option<ElementId>,
        package: Option<ElementId>,
        role: &str,
    ) -> Option<ElementId> {
        let outcome = self.resolve_reference_outcome(reference, owner, package);
        self.report_resolution_outcome(reference, outcome, role)
    }

    fn resolve_reference_guarded(
        &self,
        reference: &Reference,
        owner: Option<ElementId>,
        package: Option<ElementId>,
        state: &mut ResolutionState,
    ) -> ResolutionOutcome {
        let Some(first) = reference.segments.first() else {
            return ResolutionOutcome::missing();
        };
        let mut outcome =
            self.resolve_simple_guarded(first, owner, package, reference.span.source, state);
        if reference.form == ReferenceForm::FeatureChain || reference.segments.len() > 1 {
            for segment in reference.segments.iter().skip(1) {
                let parent = match &outcome.resolution {
                    Resolution::Found(parent) => *parent,
                    Resolution::Missing | Resolution::Ambiguous => break,
                };
                let next =
                    self.resolve_members_guarded(parent, segment, MemberAccess::Qualified, state);
                outcome = outcome.advance(next);
            }
        }
        outcome
    }

    fn resolve_simple_guarded(
        &self,
        name: &str,
        mut owner: Option<ElementId>,
        package: Option<ElementId>,
        source: SourceId,
        state: &mut ResolutionState,
    ) -> ResolutionOutcome {
        let mut visited = BTreeSet::new();
        let mut pending_failures = Vec::new();
        while let Some(scope) = owner {
            if !visited.insert(scope) {
                break;
            }
            let mut scoped = self.resolve_members_guarded(scope, name, MemberAccess::Local, state);
            if scoped.resolution != Resolution::Missing {
                scoped.extend_higher_precedence_failures(pending_failures);
                return scoped;
            }
            pending_failures.extend(scoped.failures);
            owner = self.project.element(scope).owner;
        }

        if let Some(package) = package.filter(|package| visited.insert(*package)) {
            let mut scoped =
                self.resolve_members_guarded(package, name, MemberAccess::Local, state);
            if scoped.resolution != Resolution::Missing {
                scoped.extend_higher_precedence_failures(pending_failures);
                return scoped;
            }
            pending_failures.extend(scoped.failures);
        }

        let mut root = self
            .project
            .elements
            .iter()
            .filter(|element| element.owner.is_none() && element_has_name(element, name))
            .map(|element| element.id)
            .collect::<Vec<_>>();
        dedup_ids(&mut root);
        if !root.is_empty() {
            let mut root = ResolutionOutcome::from_candidates(root);
            root.extend_higher_precedence_failures(pending_failures);
            return root;
        }

        let mut imported = self.resolve_imported_members_guarded(
            ImportLookupScope::Root(source),
            name,
            MemberAccess::Local,
            state,
        );
        imported.extend_higher_precedence_failures(pending_failures);
        imported
    }

    fn resolve_imported_members_guarded(
        &self,
        scope: ImportLookupScope,
        name: &str,
        access: MemberAccess,
        state: &mut ResolutionState,
    ) -> ResolutionOutcome {
        let mut imported = ResolutionCandidates::default();
        for import in self.project.imports.iter().filter(|import| {
            scope.contains(import)
                && !import.all
                && !import.recursive
                && !import.filtered
                && import_is_visible(import, access)
        }) {
            if import.wildcard {
                let target = self.resolve_import_target_guarded(import, state);
                let resolution = match &target.resolution {
                    Resolution::Found(imported_namespace) => {
                        let members = self.resolve_members_guarded(
                            *imported_namespace,
                            name,
                            MemberAccess::Qualified,
                            state,
                        );
                        target.advance(members)
                    }
                    Resolution::Missing | Resolution::Ambiguous => target,
                };
                imported.include(resolution);
            } else {
                let imported_name = import.reference.segments.last().map(String::as_str);
                let resolution = self.resolve_import_target_guarded(import, state);
                let exposes_name = match &resolution.resolution {
                    Resolution::Found(target) => {
                        element_has_name(self.project.element(*target), name)
                    }
                    Resolution::Ambiguous => {
                        imported_name == Some(name)
                            || resolution
                                .known_ids
                                .iter()
                                .any(|target| element_has_name(self.project.element(*target), name))
                    }
                    Resolution::Missing => imported_name == Some(name),
                };
                if exposes_name {
                    imported.include(resolution);
                }
            }
        }
        imported.finish()
    }

    fn validate_import_target_reference(&mut self, import: &Import) {
        let mut state = ResolutionState::default();
        let outcome = self.resolve_import_target_guarded(import, &mut state);
        let _ = self.report_resolution_outcome(&import.reference, outcome, "import target");
    }

    fn resolve_import_target_guarded(
        &self,
        import: &Import,
        state: &mut ResolutionState,
    ) -> ResolutionOutcome {
        let Some(first) = import.reference.segments.first() else {
            return ResolutionOutcome::missing();
        };
        let import_key = (import.span.source.0, import.span.start.byte);
        if !state.imports.insert(import_key) {
            return ResolutionOutcome::missing();
        }

        let mut outcome =
            self.resolve_simple_guarded(first, import.scope, None, import.span.source, state);
        for segment in import.reference.segments.iter().skip(1) {
            let parent = match &outcome.resolution {
                Resolution::Found(parent) => *parent,
                Resolution::Missing | Resolution::Ambiguous => break,
            };
            let next =
                self.resolve_members_guarded(parent, segment, MemberAccess::Qualified, state);
            outcome = outcome.advance(next);
        }
        state.imports.remove(&import_key);
        outcome
    }

    fn resolve_members_guarded(
        &self,
        parent: ElementId,
        name: &str,
        access: MemberAccess,
        state: &mut ResolutionState,
    ) -> ResolutionOutcome {
        let member_key = (parent, name.to_owned(), access);
        if !state.members.insert(member_key.clone()) {
            return ResolutionOutcome::missing();
        }
        let mut visited = BTreeSet::new();
        let resolution =
            self.named_members_through_types_guarded(parent, name, access, &mut visited, state);
        state.members.remove(&member_key);
        resolution
    }

    fn named_members_through_types_guarded(
        &self,
        parent: ElementId,
        name: &str,
        access: MemberAccess,
        visited: &mut BTreeSet<ElementId>,
        state: &mut ResolutionState,
    ) -> ResolutionOutcome {
        if !visited.insert(parent) {
            return ResolutionOutcome::missing();
        }
        let direct = self.named_members(parent, name);
        if !direct.is_empty() {
            return ResolutionOutcome::from_candidates(direct);
        }
        let imported = self.resolve_imported_members_guarded(
            ImportLookupScope::Owned(parent),
            name,
            access,
            state,
        );
        if imported.resolution != Resolution::Missing {
            return imported;
        }
        let import_failures = imported.failures;

        let element = self.project.element(parent);
        let mut inherited = ResolutionCandidates::default();
        for reference in element
            .declared_types
            .iter()
            .chain(element.specializations.iter())
        {
            let parent_outcome =
                self.resolve_reference_guarded(reference, element.owner, element.package, state);
            let resolution = match &parent_outcome.resolution {
                Resolution::Found(parent_type) => {
                    let members = self.named_members_through_types_guarded(
                        *parent_type,
                        name,
                        MemberAccess::Qualified,
                        visited,
                        state,
                    );
                    parent_outcome.advance(members)
                }
                Resolution::Missing | Resolution::Ambiguous => {
                    Self::parent_failure_outcome(reference, parent_outcome)
                }
            };
            inherited.include(resolution);
        }
        let mut inherited = inherited.finish();
        inherited.extend_higher_precedence_failures(import_failures);
        inherited
    }

    fn named_members(&self, parent: ElementId, name: &str) -> Vec<ElementId> {
        let mut members = self
            .project
            .elements
            .iter()
            .filter(|element| element.owner == Some(parent) && element_has_name(element, name))
            .map(|element| element.id)
            .collect::<Vec<_>>();
        dedup_ids(&mut members);
        members
    }

    fn first_resolved_type(&self, element: &Element) -> Option<ElementId> {
        element.declared_types.iter().find_map(|reference| {
            match self.resolve_reference(reference, element.owner, element.package) {
                Resolution::Found(id) => Some(id),
                _ => None,
            }
        })
    }

    fn resolved_subject_type(&self, element: &Element) -> Option<ElementId> {
        self.resolved_element_type(element.id, &mut BTreeSet::new())
    }

    fn local_subject_type(
        &self,
        subject: &Element,
        inherited_type: Option<ElementId>,
    ) -> SubjectTypeOutcome {
        if self.effective_type_chain_is_invalid(subject.id)
            || self.binding_target_type_chain_is_invalid(subject)
        {
            return SubjectTypeOutcome::Invalid;
        }

        let declared_type = self.first_resolved_type(subject);
        if !subject.declared_types.is_empty() && declared_type.is_none() {
            return SubjectTypeOutcome::Invalid;
        }
        if let (Some(declared), Some(inherited)) = (declared_type, inherited_type) {
            if self.is_same_or_specializes_without_reporting(
                declared,
                inherited,
                &mut BTreeSet::new(),
            ) != Conformance::Conforms
            {
                return SubjectTypeOutcome::Invalid;
            }
        }

        let Some(actual_type) = self.resolved_subject_type(subject) else {
            return if declared_type.or(inherited_type).is_some() {
                SubjectTypeOutcome::Invalid
            } else {
                SubjectTypeOutcome::Missing
            };
        };
        if let Some(expected_type) = declared_type.or(inherited_type) {
            if self.is_same_or_specializes_without_reporting(
                actual_type,
                expected_type,
                &mut BTreeSet::new(),
            ) != Conformance::Conforms
            {
                return SubjectTypeOutcome::Invalid;
            }
        }
        SubjectTypeOutcome::Resolved(actual_type)
    }

    fn resolved_element_type(
        &self,
        element_id: ElementId,
        visited: &mut BTreeSet<ElementId>,
    ) -> Option<ElementId> {
        if !visited.insert(element_id) {
            return None;
        }
        let element = self.project.element(element_id);
        if let Some(binding) = &element.referenced_feature {
            let Resolution::Found(bound_element) =
                self.resolve_reference(binding, element.owner, element.package)
            else {
                return None;
            };
            return self.resolved_element_type(bound_element, visited);
        }
        if has_multiple_parent_relationships(element) {
            return None;
        }
        if let Some(declared_type) = self.first_resolved_type(element) {
            return Some(declared_type);
        }
        let specialization = element.specializations.first()?;
        let Resolution::Found(parent) =
            self.resolve_reference(specialization, element.owner, element.package)
        else {
            return None;
        };
        if self.project.element(parent).kind.is_usage() {
            self.resolved_element_type(parent, visited)
        } else {
            Some(parent)
        }
    }

    fn inherited_requirement_subject_type(&self, requirement_id: ElementId) -> Option<ElementId> {
        let element = self.project.element(requirement_id);
        if has_multiple_parent_relationships(element) {
            return None;
        }
        self.resolved_requirement_parents(element)
            .into_iter()
            .find_map(|parent| self.effective_requirement_subject_type(parent))
    }

    fn effective_requirement_subject_type(&self, requirement_id: ElementId) -> Option<ElementId> {
        self.effective_requirement_subject_type_inner(requirement_id, &mut BTreeSet::new())
    }

    fn effective_requirement_subject_type_inner(
        &self,
        requirement_id: ElementId,
        visited: &mut BTreeSet<ElementId>,
    ) -> Option<ElementId> {
        if !visited.insert(requirement_id) {
            return None;
        }
        let requirement = self
            .project
            .requirements
            .iter()
            .find(|requirement| requirement.element == requirement_id)?;
        let element = self.project.element(requirement_id);
        if requirement.subjects.len() > 1 || has_multiple_parent_relationships(element) {
            return None;
        }
        let inherited_type = self
            .resolved_requirement_parents(element)
            .into_iter()
            .find_map(|parent| self.effective_requirement_subject_type_inner(parent, visited));
        if let Some(subject) = requirement.subjects.first() {
            let inherited_constraint = element
                .kind
                .is_requirement_usage()
                .then_some(inherited_type)
                .flatten();
            return match self
                .local_subject_type(self.project.element(*subject), inherited_constraint)
            {
                SubjectTypeOutcome::Resolved(subject_type) => Some(subject_type),
                SubjectTypeOutcome::Missing | SubjectTypeOutcome::Invalid => None,
            };
        }
        inherited_type
    }

    fn resolved_requirement_parents(&self, element: &Element) -> Vec<ElementId> {
        let mut parents = Vec::new();
        for reference in &element.declared_types {
            let Resolution::Found(parent) =
                self.resolve_reference(reference, element.owner, element.package)
            else {
                continue;
            };
            if self.project.element(parent).kind == ElementKind::RequirementDefinition {
                parents.push(parent);
            }
        }

        for reference in &element.specializations {
            let Resolution::Found(parent) =
                self.resolve_reference(reference, element.owner, element.package)
            else {
                continue;
            };
            let parent_kind = self.project.element(parent).kind;
            let valid = if element.kind.is_requirement_usage() {
                parent_kind.is_requirement_usage()
            } else {
                parent_kind == ElementKind::RequirementDefinition
            };
            if valid {
                parents.push(parent);
            }
        }
        parents
    }

    fn requirement_parent_chain_is_cyclic(&self, start: ElementId) -> bool {
        let mut current = start;
        let mut visited = BTreeSet::new();
        loop {
            if !visited.insert(current) {
                return current == start;
            }
            let element = self.project.element(current);
            if has_multiple_parent_relationships(element) {
                return false;
            }
            let parents = self.resolved_requirement_parents(element);
            let [parent] = parents.as_slice() else {
                return false;
            };
            current = *parent;
        }
    }

    fn inherited_verification_subject_type(&self, verification_id: ElementId) -> Option<ElementId> {
        let element = self.project.element(verification_id);
        if has_multiple_parent_relationships(element) {
            return None;
        }
        self.resolved_verification_parents(element)
            .into_iter()
            .find_map(|parent| self.effective_verification_subject_type(parent))
    }

    fn effective_verification_subject_type(&self, verification_id: ElementId) -> Option<ElementId> {
        match self.effective_verification_subject_type_outcome(verification_id) {
            SubjectTypeOutcome::Resolved(subject_type) => Some(subject_type),
            SubjectTypeOutcome::Missing | SubjectTypeOutcome::Invalid => None,
        }
    }

    fn effective_verification_subject_type_outcome(
        &self,
        verification_id: ElementId,
    ) -> SubjectTypeOutcome {
        self.effective_verification_subject_type_inner(verification_id, &mut BTreeSet::new())
    }

    fn effective_verification_subject_type_inner(
        &self,
        verification_id: ElementId,
        visited: &mut BTreeSet<ElementId>,
    ) -> SubjectTypeOutcome {
        if !visited.insert(verification_id) {
            return SubjectTypeOutcome::Invalid;
        }
        let Some(verification) = self
            .project
            .verifications
            .iter()
            .find(|verification| verification.element == verification_id)
        else {
            return SubjectTypeOutcome::Invalid;
        };
        let element = self.project.element(verification_id);
        if has_multiple_parent_relationships(element) {
            return SubjectTypeOutcome::Invalid;
        }
        let parents = self.resolved_verification_parents(element);
        let inherited_type = match parents.as_slice() {
            [parent] => self.effective_verification_subject_type_inner(*parent, visited),
            [] if element.declared_types.is_empty() && element.specializations.is_empty() => {
                SubjectTypeOutcome::Missing
            }
            [] | [_, _, ..] => SubjectTypeOutcome::Invalid,
        };
        if let Some(subject) = verification.subjects.first() {
            return match inherited_type {
                SubjectTypeOutcome::Resolved(inherited_type) => {
                    self.local_subject_type(self.project.element(*subject), Some(inherited_type))
                }
                SubjectTypeOutcome::Missing => {
                    self.local_subject_type(self.project.element(*subject), None)
                }
                SubjectTypeOutcome::Invalid => SubjectTypeOutcome::Invalid,
            };
        }
        inherited_type
    }

    fn resolved_verification_parents(&self, element: &Element) -> Vec<ElementId> {
        let mut parents = Vec::new();
        for reference in &element.declared_types {
            let Resolution::Found(parent) =
                self.resolve_reference(reference, element.owner, element.package)
            else {
                continue;
            };
            if self.project.element(parent).kind == ElementKind::VerificationDefinition {
                parents.push(parent);
            }
        }

        for reference in &element.specializations {
            let Resolution::Found(parent) =
                self.resolve_reference(reference, element.owner, element.package)
            else {
                continue;
            };
            let parent_kind = self.project.element(parent).kind;
            let valid = if element.kind == ElementKind::VerificationUsage {
                parent_kind == ElementKind::VerificationUsage
            } else {
                parent_kind == ElementKind::VerificationDefinition
            };
            if valid {
                parents.push(parent);
            }
        }
        parents
    }

    fn verification_parent_chain_is_cyclic(&self, start: ElementId) -> bool {
        let mut current = start;
        let mut visited = BTreeSet::new();
        loop {
            if !visited.insert(current) {
                return current == start;
            }
            let element = self.project.element(current);
            if has_multiple_parent_relationships(element) {
                return false;
            }
            let parents = self.resolved_verification_parents(element);
            let [parent] = parents.as_slice() else {
                return false;
            };
            current = *parent;
        }
    }

    fn is_same_or_specializes(
        &mut self,
        actual: ElementId,
        expected: ElementId,
        visited: &mut BTreeSet<ElementId>,
    ) -> Conformance {
        if actual == expected {
            return Conformance::Conforms;
        }
        if !visited.insert(actual) {
            return Conformance::DoesNotConform;
        }
        let element = self.project.element(actual);
        let owner = element.owner;
        let package = element.package;
        let specializations = element.specializations.clone();
        let mut conforms = false;
        let mut indeterminate = false;
        for reference in &specializations {
            let Some(parent) =
                self.resolve_profile_reference(reference, owner, package, "specialization parent")
            else {
                indeterminate = true;
                continue;
            };
            match self.is_same_or_specializes(parent, expected, visited) {
                Conformance::Conforms => conforms = true,
                Conformance::DoesNotConform => {}
                Conformance::Indeterminate => indeterminate = true,
            }
        }

        if conforms {
            Conformance::Conforms
        } else if indeterminate {
            Conformance::Indeterminate
        } else {
            Conformance::DoesNotConform
        }
    }

    fn is_same_or_specializes_without_reporting(
        &self,
        actual: ElementId,
        expected: ElementId,
        visited: &mut BTreeSet<ElementId>,
    ) -> Conformance {
        if actual == expected {
            return Conformance::Conforms;
        }
        if !visited.insert(actual) {
            return Conformance::DoesNotConform;
        }
        let element = self.project.element(actual);
        let mut conforms = false;
        let mut indeterminate = false;
        for reference in &element.specializations {
            let Resolution::Found(parent) =
                self.resolve_reference(reference, element.owner, element.package)
            else {
                indeterminate = true;
                continue;
            };
            match self.is_same_or_specializes_without_reporting(parent, expected, visited) {
                Conformance::Conforms => conforms = true,
                Conformance::DoesNotConform => {}
                Conformance::Indeterminate => indeterminate = true,
            }
        }

        if conforms {
            Conformance::Conforms
        } else if indeterminate {
            Conformance::Indeterminate
        } else {
            Conformance::DoesNotConform
        }
    }

    fn push_resolution_once(&mut self, span: Span, code: &'static str, message: impl Into<String>) {
        if self
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.span == span && diagnostic.code == code)
        {
            return;
        }
        self.push(span, code, message);
    }

    fn push(&mut self, span: Span, code: &'static str, message: impl Into<String>) {
        self.diagnostics.push(SemanticDiagnostic {
            span,
            code,
            message: message.into(),
        });
    }
}

fn import_is_visible(import: &Import, access: MemberAccess) -> bool {
    match (import.visibility, access) {
        (ImportVisibility::Private, MemberAccess::Local) => true,
        (ImportVisibility::Public, MemberAccess::Local | MemberAccess::Qualified) => {
            import.scope.is_some()
        }
        _ => false,
    }
}

fn dedup_ids(ids: &mut Vec<ElementId>) {
    ids.sort();
    ids.dedup();
}

fn element_has_name(element: &Element, name: &str) -> bool {
    element.name.as_deref() == Some(name) || element.short_name.as_deref() == Some(name)
}

fn has_multiple_parent_relationships(element: &Element) -> bool {
    element.declared_types.len() > 1
        || element.specializations.len() > 1
        || (!element.declared_types.is_empty() && !element.specializations.is_empty())
}

fn unsupported_parent_reference(element: &Element) -> Option<&Reference> {
    if element.declared_types.len() > 1 {
        element.declared_types.get(1)
    } else if !element.declared_types.is_empty() && !element.specializations.is_empty() {
        element.specializations.first()
    } else if element.specializations.len() > 1 {
        element.specializations.get(1)
    } else {
        None
    }
}

fn kind_name(kind: ElementKind) -> &'static str {
    match kind {
        ElementKind::Package => "package",
        ElementKind::PartDefinition => "part definition",
        ElementKind::PartUsage => "part usage",
        ElementKind::RequirementDefinition => "requirement definition",
        ElementKind::RequirementUsage => "requirement usage",
        ElementKind::ConstraintDefinition => "constraint definition",
        ElementKind::ConstraintUsage => "constraint usage",
        ElementKind::VerificationDefinition => "verification definition",
        ElementKind::VerificationUsage => "verification usage",
        ElementKind::Objective => "objective",
        ElementKind::Subject => "subject",
        ElementKind::Actor => "actor",
        ElementKind::Stakeholder => "stakeholder",
        ElementKind::GenericDefinition => "definition",
        ElementKind::GenericUsage => "usage",
    }
}

fn merge_diagnostics(
    project: &Project,
    files: &mut [CheckFileReport],
    diagnostics: Vec<SemanticDiagnostic>,
) {
    let mut by_path = BTreeMap::<String, Vec<CheckDiagnostic>>::new();
    for diagnostic in diagnostics {
        let path = project.source(diagnostic.span.source).path.clone();
        by_path.entry(path).or_default().push(CheckDiagnostic {
            severity: "error".to_owned(),
            code: diagnostic.code.to_owned(),
            message: diagnostic.message,
            span: CheckSpan {
                start_line: diagnostic.span.start.line,
                start_column: diagnostic.span.start.column,
                end_line: diagnostic.span.end.line,
                end_column: diagnostic.span.end.column,
            },
        });
    }

    for file in files {
        if let Some(mut additions) = by_path.remove(&file.path) {
            file.diagnostics.append(&mut additions);
            file.diagnostics.sort_by(|left, right| {
                left.span
                    .start_line
                    .cmp(&right.span.start_line)
                    .then_with(|| left.span.start_column.cmp(&right.span.start_column))
                    .then_with(|| left.code.cmp(&right.code))
                    .then_with(|| left.message.cmp(&right.message))
            });
        }
        file.valid = file.diagnostics.is_empty();
    }
}
