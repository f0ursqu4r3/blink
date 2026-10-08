//! App-local attachments for disk-backed projects.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use serde::{Deserialize, Serialize};

use crate::environments::root_group;
use crate::ids;
use crate::model::{AuthorizationConfig, Definitions, Draft};
use crate::project::{ProjectDocument, ProjectRequest};
use crate::session::create_session;
use crate::workspace_state::Workspace;

/// Local metadata. This structure is never written to a project folder.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectAttachment {
    pub root_id: u64,
    pub path: String,
    pub disk_root_id: u64,
    #[serde(default)]
    pub disk_baseline: Option<ProjectDocument>,
    #[serde(default)]
    pub private_fields: ProjectPrivateFields,
    pub group_ids: BTreeMap<u64, u64>,
    pub request_ids: BTreeMap<u64, u64>,
    pub environment_ids: BTreeMap<u64, u64>,
    pub token_ids: BTreeMap<u64, u64>,
    /// Captures keyed by runtime environment (zero denotes base values).
    #[serde(default)]
    pub private_values: BTreeMap<u64, Definitions>,
}

/// Permanent privacy classifications. Removing an auth consumer does not
/// authorize publication of its former credential values.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectPrivateFields {
    definition_names: BTreeSet<String>,
    headers: BTreeSet<(u64, u64)>,
}

impl ProjectAttachment {
    fn remember_private_fields(&mut self, document: &ProjectDocument) {
        let mut privacy = self.private_fields.clone();
        if let Some(previous) = &self.disk_baseline {
            privacy =
                sanitize_credentials(&mut previous.clone(), &mut Definitions::new(), &privacy);
        }
        self.private_fields =
            sanitize_credentials(&mut document.clone(), &mut Definitions::new(), &privacy);
    }

    /// Retain classifications from both baselines before accepting a disk save.
    pub fn accept_disk_baseline(&mut self, document: ProjectDocument) {
        self.remember_private_fields(&document);
        self.disk_baseline = Some(document);
    }
}

/// A closed project's private session backup. Never copied into project files.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClosedProject {
    pub path: String,
    pub snapshot: String,
}

fn disk_id(map: &BTreeMap<u64, u64>, runtime: u64) -> u64 {
    map.iter()
        .find(|(_, value)| **value == runtime)
        .map(|(key, _)| *key)
        .unwrap_or(runtime)
}

fn runtime_id(map: &mut BTreeMap<u64, u64>, disk: u64, sequence: &ids::Sequence) -> u64 {
    *map.entry(disk).or_insert_with(|| sequence.next())
}

fn private_text(value: &mut String, name: String, secrets: &mut Definitions) {
    if value.is_empty()
        || crate::interpolation::TOKEN_RE
            .find(value)
            .is_some_and(|m| m.start() == 0 && m.end() == value.len())
    {
        return;
    }
    secrets.insert(name.clone(), std::mem::take(value));
    *value = format!("{{{{{name}}}}}");
}

fn private_auth(auth: &mut Option<AuthorizationConfig>, prefix: &str, secrets: &mut Definitions) {
    match auth {
        Some(AuthorizationConfig::Bearer { token }) => {
            private_text(token, format!("{prefix}_token"), secrets)
        }
        Some(AuthorizationConfig::Basic { username, password }) => {
            private_text(username, format!("{prefix}_username"), secrets);
            private_text(password, format!("{prefix}_password"), secrets);
        }
        _ => {}
    }
}

fn private_draft(draft: &mut Draft, id: u64, secrets: &mut Definitions) {
    let prefix = format!("blink_private_request_{id}");
    private_auth(&mut draft.local_auth, &prefix, secrets);
    private_text(&mut draft.token, format!("{prefix}_flat_token"), secrets);
    private_text(
        &mut draft.username,
        format!("{prefix}_flat_username"),
        secrets,
    );
    private_text(
        &mut draft.password,
        format!("{prefix}_flat_password"),
        secrets,
    );
}

fn restore_text(value: &mut String, secrets: &Definitions) {
    if let Some(name) = value.strip_prefix("{{").and_then(|s| s.strip_suffix("}}"))
        && let Some(secret) = secrets.get(name)
    {
        *value = secret.clone();
    }
}

fn restore_auth(auth: &mut Option<AuthorizationConfig>, secrets: &Definitions) {
    match auth {
        Some(AuthorizationConfig::Bearer { token }) => restore_text(token, secrets),
        Some(AuthorizationConfig::Basic { username, password }) => {
            restore_text(username, secrets);
            restore_text(password, secrets);
        }
        _ => {}
    }
}

fn sanitize_credentials(
    doc: &mut ProjectDocument,
    secrets: &mut Definitions,
    retained: &ProjectPrivateFields,
) -> ProjectPrivateFields {
    let credential_header = |name: &str| {
        [
            "authorization",
            "proxy-authorization",
            "cookie",
            "x-api-key",
        ]
        .iter()
        .any(|key| name.eq_ignore_ascii_case(key))
    };
    let mut names = HashSet::new();
    let mut privacy = retained.clone();
    let mut pending: Vec<String> = doc
        .groups
        .iter()
        .filter_map(|g| g.local_auth.as_ref())
        .map(|a| serde_json::to_string(a).unwrap_or_default())
        .collect();
    pending.extend(
        retained
            .definition_names
            .iter()
            .map(|name| format!("{{{{{name}}}}}")),
    );
    // Older snapshots can recover classification from generated private references.
    for group in &doc.groups {
        for (name, value) in group.local_definitions.iter().flat_map(|d| d.iter()).chain(
            group
                .environments
                .iter()
                .flatten()
                .flat_map(|e| e.values.iter()),
        ) {
            if value.starts_with("{{blink_private_") {
                pending.push(format!("{{{{{name}}}}}"));
            }
        }
    }
    for request in &doc.requests {
        for header in &request.draft.headers {
            if credential_header(&header.key) || header.value.starts_with("{{blink_private_") {
                privacy.headers.insert((request.id, header.id));
            }
        }
        if let Some(auth) = &request.draft.local_auth {
            pending.push(serde_json::to_string(auth).unwrap_or_default());
        }
        pending.extend([
            request.draft.token.clone(),
            request.draft.username.clone(),
            request.draft.password.clone(),
        ]);
        pending.extend(
            request
                .draft
                .headers
                .iter()
                .filter(|h| privacy.headers.contains(&(request.id, h.id)))
                .map(|h| h.value.clone()),
        );
    }
    while let Some(text) = pending.pop() {
        for capture in crate::interpolation::TOKEN_RE.captures_iter(&text) {
            let name = capture[2].to_string();
            if names.insert(name.clone()) {
                for group in &doc.groups {
                    if let Some(value) = group.local_definitions.as_ref().and_then(|d| d.get(&name))
                    {
                        pending.push(value.clone());
                    }
                    for environment in group.environments.iter().flatten() {
                        if let Some(value) = environment.values.get(&name) {
                            pending.push(value.clone());
                        }
                    }
                }
            }
        }
    }
    for group in &mut doc.groups {
        private_auth(
            &mut group.local_auth,
            &format!("blink_private_group_{}", group.id),
            secrets,
        );
        if let Some(definitions) = &mut group.local_definitions {
            for (name, value) in definitions {
                if names.contains(name) {
                    private_text(
                        value,
                        format!("blink_private_group_{}_definition_{name}", group.id),
                        secrets,
                    );
                }
            }
        }
        for environment in group.environments.iter_mut().flatten() {
            for (name, value) in &mut environment.values {
                if names.contains(name) {
                    private_text(
                        value,
                        format!(
                            "blink_private_group_{}_environment_{}_definition_{name}",
                            group.id, environment.id
                        ),
                        secrets,
                    );
                }
            }
        }
    }
    for request in &mut doc.requests {
        private_draft(&mut request.draft, request.id, secrets);
        for header in &mut request.draft.headers {
            if privacy.headers.contains(&(request.id, header.id)) {
                private_text(
                    &mut header.value,
                    format!("blink_private_request_{}_header_{}", request.id, header.id),
                    secrets,
                );
            }
        }
    }
    privacy.definition_names.extend(names);
    privacy
}

impl Workspace {
    pub fn project_for_group(&self, group_id: Option<u64>) -> Option<&ProjectAttachment> {
        let root = root_group(group_id, &self.groups)?;
        self.projects
            .iter()
            .find(|project| project.root_id == root.id)
    }

    pub fn can_move_request(&self, id: u64, target: Option<u64>) -> bool {
        let Some(session) = self.session(id) else {
            return false;
        };
        !target.is_some_and(|id| self.group(id).is_none())
            && self.project_for_group(session.group_id).map(|p| p.root_id)
                == self.project_for_group(target).map(|p| p.root_id)
    }

    pub fn can_move_group(&self, id: u64, target: Option<u64>) -> bool {
        let Some(group) = self.group(id) else {
            return false;
        };
        if target.is_some_and(|target| self.group(target).is_none()) {
            return false;
        }
        if self.projects.iter().any(|p| p.root_id == id) {
            return target.is_none();
        }
        self.project_for_group(Some(group.id)).map(|p| p.root_id)
            == self.project_for_group(target).map(|p| p.root_id)
    }

    /// Explicit transfer, after the caller discloses the destination folder.
    pub fn transfer_requests(&mut self, ids: &[u64], target: Option<u64>) -> Result<(), String> {
        if target.is_some_and(|id| self.group(id).is_none()) {
            return Err("Destination group does not exist.".into());
        }
        let moved: HashSet<u64> = ids.iter().copied().collect();
        for id in &moved {
            let session = self.session(*id).ok_or("Request does not exist.")?;
            if session.running() {
                return Err("Stop running requests before transferring them.".into());
            }
        }
        let mut next = self.clone();
        for session in &mut next.sessions {
            if moved.contains(&session.id) {
                session.group_id = target;
            }
        }
        for group in &next.groups {
            for token in group.response_tokens.iter().flatten() {
                if moved.contains(&token.request_id)
                    && next.session(token.request_id).is_some_and(|s| {
                        next.project_for_group(s.group_id).map(|p| p.root_id)
                            != next.project_for_group(Some(group.id)).map(|p| p.root_id)
                    })
                {
                    return Err("A response token reads a transferred request. Remove that dependency before transferring.".into());
                }
            }
        }
        if next
            .global_response_tokens
            .iter()
            .any(|t| moved.contains(&t.request_id))
            && next.project_for_group(target).is_some()
        {
            return Err("A workspace response token reads a transferred request. Remove that dependency first.".into());
        }
        {
            let before = self.token_sources(0.0);
            let after = next.token_sources(0.0);
            for id in &moved {
                let previous = self.session(*id).unwrap();
                let following = next.session(*id).unwrap();
                let a = before.request_context(previous);
                let b = after.request_context(following);
                if a.tokens.response_tokens != b.tokens.response_tokens
                    || a.tokens.workspace_response_tokens != b.tokens.workspace_response_tokens
                {
                    return Err("Transfer changes response-token dependencies. Remove those dependencies or define the same sources in the destination first.".into());
                }
                let same = a == b
                    || (a.auth == b.auth
                        && crate::request::build_request(
                            &previous.draft,
                            Some(crate::request::RequestContext::Resolved(&a)),
                        )
                        .ok()
                        .zip(
                            crate::request::build_request(
                                &following.draft,
                                Some(crate::request::RequestContext::Resolved(&b)),
                            )
                            .ok(),
                        )
                        .is_some_and(|(a, b)| a == b));
                if !same {
                    return Err("Transfer changes inherited authorization or token values. Set explicit authorization and define the required tokens in the destination first.".into());
                }
            }
        }
        let destination = next.project_for_group(target).map(|p| p.root_id);
        let changed_scope: HashSet<u64> = moved
            .iter()
            .copied()
            .filter(|id| {
                self.project_for_group(self.session(*id).unwrap().group_id)
                    .map(|p| p.root_id)
                    != destination
            })
            .collect();
        for project in &mut next.projects {
            project
                .request_ids
                .retain(|_, runtime| !changed_scope.contains(runtime));
            if Some(project.root_id) == destination {
                for id in &changed_scope {
                    project.request_ids.insert(ids::SESSIONS.next(), *id);
                }
            }
        }
        next.discard_deletion();
        *self = next;
        Ok(())
    }

    pub fn attach_project(&mut self, path: String, doc: ProjectDocument) -> Result<u64, String> {
        if let Some(index) = self.closed_projects.iter().position(|p| p.path == path) {
            let mut archived = Workspace::decode(&self.closed_projects[index].snapshot)?;
            let root = archived
                .projects
                .first()
                .ok_or("Invalid closed project archive.")?
                .root_id;
            archived.reload_project(root, doc)?;
            if archived.groups.iter().any(|g| self.group(g.id).is_some())
                || archived
                    .sessions
                    .iter()
                    .any(|s| self.session(s.id).is_some())
            {
                return Err("Closed project IDs conflict with this workspace.".into());
            }
            self.groups.extend(archived.groups);
            self.sessions.extend(
                archived
                    .sessions
                    .into_iter()
                    .filter(|s| s.group_id.is_some()),
            );
            self.projects.extend(archived.projects);
            self.closed_projects.remove(index);
            self.discard_deletion();
            return Ok(root);
        }
        if self.projects.iter().any(|p| p.path == path) {
            return Err("This project folder is already open.".into());
        }
        let mut next = self.clone();
        let attachment = ProjectAttachment {
            root_id: 0,
            path,
            disk_root_id: doc.root_id,
            disk_baseline: Some(doc.clone()),
            private_fields: ProjectPrivateFields::default(),
            group_ids: BTreeMap::new(),
            request_ids: BTreeMap::new(),
            environment_ids: BTreeMap::new(),
            token_ids: BTreeMap::new(),
            private_values: BTreeMap::new(),
        };
        next.projects.push(attachment);
        next.install_project(next.projects.len() - 1, doc, &Definitions::new())?;
        let root = next.projects.last().unwrap().root_id;
        next.discard_deletion();
        *self = next;
        Ok(root)
    }

    pub fn convert_group_to_project(
        &mut self,
        root_id: u64,
        path: String,
    ) -> Result<ProjectDocument, String> {
        let root = self.group(root_id).ok_or("Group does not exist.")?;
        if root.parent_id.is_some() || self.project_for_group(Some(root_id)).is_some() {
            return Err("Choose a local root group.".into());
        }
        if self.projects.iter().any(|p| p.path == path) {
            return Err("This project folder is already open.".into());
        }
        if self.sessions.iter().any(|session| {
            session.running()
                && root_group(session.group_id, &self.groups).is_some_and(|g| g.id == root_id)
        }) {
            return Err("Stop running group requests before saving as a project.".into());
        }
        let sources = self.token_sources(0.0);
        for group in &self.groups {
            if root_group(Some(group.id), &self.groups).is_some_and(|g| g.id == root_id) {
                let context = sources.context(Some(group.id));
                let mut texts = vec![serde_json::to_string(group).unwrap_or_default()];
                texts.extend(
                    self.sessions
                        .iter()
                        .filter(|s| s.group_id == Some(group.id))
                        .map(|s| serde_json::to_string(&s.draft).unwrap_or_default()),
                );
                if texts.iter().any(|text| {
                    crate::interpolation::TOKEN_RE
                        .captures_iter(text)
                        .any(|capture| {
                            capture.get(1).is_some()
                                || (self.global_definitions.contains_key(&capture[2])
                                    && !context.definitions.contains_key(&capture[2]))
                                || (self
                                    .global_response_tokens
                                    .iter()
                                    .any(|t| t.name == capture[2])
                                    && !context.definitions.contains_key(&capture[2])
                                    && !context.response_tokens.contains_key(&capture[2]))
                        })
                }) {
                    return Err("This group uses workspace-global tokens. Define those tokens in the group or its environments before saving a project.".into());
                }
            }
        }
        let mut next = self.clone();
        next.projects.push(ProjectAttachment {
            root_id,
            path,
            disk_root_id: root_id,
            disk_baseline: None,
            private_fields: ProjectPrivateFields::default(),
            group_ids: BTreeMap::new(),
            request_ids: BTreeMap::new(),
            environment_ids: BTreeMap::new(),
            token_ids: BTreeMap::new(),
            private_values: BTreeMap::new(),
        });
        let mut capture_names = next
            .local_capture_names
            .get(&root_id)
            .cloned()
            .unwrap_or_default();
        for session in &next.sessions {
            if next
                .project_for_group(session.group_id)
                .is_some_and(|p| p.root_id == root_id)
            {
                capture_names.extend(
                    session
                        .draft
                        .captures
                        .iter()
                        .flatten()
                        .map(|c| c.name.clone()),
                );
            }
        }
        let mut private_values = BTreeMap::<u64, Definitions>::new();
        let root = next.groups.iter_mut().find(|g| g.id == root_id).unwrap();
        if let Some(definitions) = &mut root.local_definitions {
            for name in &capture_names {
                if let Some(value) = definitions.shift_remove(name) {
                    private_values
                        .entry(0)
                        .or_default()
                        .insert(name.clone(), value);
                }
            }
        }
        for environment in root.environments.iter_mut().flatten() {
            for name in &capture_names {
                if let Some(value) = environment.values.shift_remove(name) {
                    private_values
                        .entry(environment.id)
                        .or_default()
                        .insert(name.clone(), value);
                }
            }
        }
        next.projects.last_mut().unwrap().private_values = private_values;
        let doc = next.project_document(root_id)?;
        doc.validate()?;
        // Incoming references also become invalid when a local source changes scope.
        for group in &next.groups {
            if next.project_for_group(Some(group.id)).map(|p| p.root_id) != Some(root_id)
                && group
                    .response_tokens
                    .iter()
                    .flatten()
                    .any(|t| doc.requests.iter().any(|r| r.id == t.request_id))
            {
                return Err("A response token outside this group reads one of its requests. Remove the dependency first.".into());
            }
        }
        if next
            .global_response_tokens
            .iter()
            .any(|t| doc.requests.iter().any(|r| r.id == t.request_id))
        {
            return Err("A global response token reads a request in this group. Remove the dependency first.".into());
        }
        next.projects
            .last_mut()
            .unwrap()
            .accept_disk_baseline(doc.clone());
        next.discard_deletion();
        *self = next;
        Ok(doc)
    }

    fn project_projection(
        &self,
        root_id: u64,
        validate_references: bool,
    ) -> Result<(ProjectDocument, Definitions), String> {
        let project = self
            .projects
            .iter()
            .find(|p| p.root_id == root_id)
            .ok_or("Project is not open.")?;
        let group_ids: HashSet<u64> = self
            .groups
            .iter()
            .filter(|g| self.project_for_group(Some(g.id)).map(|p| p.root_id) == Some(root_id))
            .map(|g| g.id)
            .collect();
        let request_ids: HashSet<u64> = self
            .sessions
            .iter()
            .filter(|s| s.group_id.is_some_and(|id| group_ids.contains(&id)))
            .map(|s| s.id)
            .collect();
        let mut secrets = Definitions::new();
        let mut groups = Vec::new();
        for original in self.groups.iter().filter(|g| group_ids.contains(&g.id)) {
            let mut group = original.clone();
            group.id = disk_id(&project.group_ids, group.id);
            group.parent_id = group.parent_id.map(|id| disk_id(&project.group_ids, id));
            group.collapsed = false;
            group.active_environment_id = None;
            for environment in group.environments.iter_mut().flatten() {
                environment.id = disk_id(&project.environment_ids, environment.id);
            }
            for token in group.response_tokens.iter_mut().flatten() {
                if validate_references && !request_ids.contains(&token.request_id) {
                    return Err(
                        "Project response tokens must read requests in the same project.".into(),
                    );
                }
                token.id = disk_id(&project.token_ids, token.id);
                token.request_id = disk_id(&project.request_ids, token.request_id);
            }
            groups.push(group);
        }
        let requests = self
            .sessions
            .iter()
            .filter(|s| request_ids.contains(&s.id))
            .map(|s| {
                let id = disk_id(&project.request_ids, s.id);
                let mut draft = s.draft.clone();
                let relative_upload = |value: &mut String| {
                    let path = std::path::Path::new(value);
                    if path.is_absolute()
                        && let Ok(relative) = path.strip_prefix(&project.path)
                        && !relative.as_os_str().is_empty()
                    {
                        *value = relative.to_string_lossy().into_owned();
                    }
                };
                if let Some(path) = &mut draft.body_file {
                    relative_upload(path);
                }
                for part in draft.form.iter_mut().flatten() {
                    if part.file == Some(true) {
                        relative_upload(&mut part.value);
                    }
                }
                ProjectRequest {
                    id,
                    group_id: disk_id(&project.group_ids, s.group_id.unwrap()),
                    draft,
                }
            })
            .collect();
        let mut doc = ProjectDocument {
            root_id: project.disk_root_id,
            groups,
            requests,
        };
        let mut privacy = project.private_fields.clone();
        if let Some(baseline) = &project.disk_baseline {
            privacy =
                sanitize_credentials(&mut baseline.clone(), &mut Definitions::new(), &privacy);
        }
        sanitize_credentials(&mut doc, &mut secrets, &privacy);
        Ok((doc, secrets))
    }

    /// Learn privacy classifications synchronously before queuing a snapshot.
    /// This does not accept edits as the saved disk baseline. Invalid response
    /// token dependencies must not prevent credential classification.
    pub fn remember_project_privacy(&mut self) {
        for index in 0..self.projects.len() {
            let root = self.projects[index].root_id;
            if let Ok((document, _)) = self.project_projection(root, false) {
                self.projects[index].remember_private_fields(&document);
            }
        }
    }

    pub fn project_document(&self, root_id: u64) -> Result<ProjectDocument, String> {
        self.project_projection(root_id, true).map(|(doc, _)| doc)
    }

    pub fn reload_project(&mut self, root_id: u64, doc: ProjectDocument) -> Result<(), String> {
        self.ensure_project_idle(root_id)?;
        let index = self
            .projects
            .iter()
            .position(|p| p.root_id == root_id)
            .ok_or("Project is not open.")?;
        let (_, secrets) = self.project_projection(root_id, false)?;
        let mut next = self.clone();
        next.install_project(index, doc, &secrets)?;
        next.discard_deletion();
        *self = next;
        Ok(())
    }

    fn ensure_project_idle(&self, root_id: u64) -> Result<(), String> {
        if !self.projects.iter().any(|p| p.root_id == root_id) {
            return Err("Project is not open.".into());
        }
        if self.sessions.iter().any(|s| {
            s.running()
                && self
                    .project_for_group(s.group_id)
                    .is_some_and(|p| p.root_id == root_id)
        }) {
            return Err("Stop running project requests first.".into());
        }
        Ok(())
    }

    pub fn close_project(&mut self, root_id: u64) -> Result<(), String> {
        self.ensure_project_idle(root_id)?;
        let groups: HashSet<u64> = self
            .groups
            .iter()
            .filter(|g| {
                self.project_for_group(Some(g.id))
                    .is_some_and(|p| p.root_id == root_id)
            })
            .map(|g| g.id)
            .collect();
        let requests: Vec<u64> = self
            .sessions
            .iter()
            .filter(|s| s.group_id.is_some_and(|id| groups.contains(&id)))
            .map(|s| s.id)
            .collect();
        let mut archive = self.clone();
        archive.groups.retain(|g| groups.contains(&g.id));
        archive.sessions.retain(|s| requests.contains(&s.id));
        archive.projects.retain(|p| p.root_id == root_id);
        archive.closed_projects.clear();
        if archive.sessions.is_empty() {
            archive.sessions.push(create_session(None));
        }
        archive.global_definitions.clear();
        archive.global_response_tokens.clear();
        archive.open_ids.clear();
        archive.active_id = None;
        let path = archive.projects[0].path.clone();
        self.closed_projects.retain(|p| p.path != path);
        self.closed_projects.push(ClosedProject {
            path,
            snapshot: archive.encode(),
        });
        self.close_tabs(&requests);
        self.delete_requests(&requests);
        self.discard_deletion();
        self.groups.retain(|g| !groups.contains(&g.id));
        self.projects.retain(|p| p.root_id != root_id);
        if self
            .focused_group_id()
            .is_some_and(|id| groups.contains(&id))
        {
            self.unfocus(false);
        }
        Ok(())
    }

    fn install_project(
        &mut self,
        index: usize,
        doc: ProjectDocument,
        secrets: &Definitions,
    ) -> Result<(), String> {
        doc.validate()?;
        self.projects[index].accept_disk_baseline(doc.clone());
        let old_root = self.projects[index].root_id;
        if old_root != 0 && self.projects[index].disk_root_id != doc.root_id {
            return Err("Project root ID changed. Close and open the project again.".into());
        }
        let old_groups: HashSet<u64> = self
            .groups
            .iter()
            .filter(|g| {
                self.project_for_group(Some(g.id))
                    .is_some_and(|p| p.root_id == old_root)
            })
            .map(|g| g.id)
            .collect();
        // Callers can supply a workspace assembled without the ID constructors.
        for group in &self.groups {
            ids::GROUPS.reserve(group.id);
            for env in group.environments.iter().flatten() {
                ids::ENVIRONMENTS.reserve(env.id);
            }
            for token in group.response_tokens.iter().flatten() {
                ids::RESPONSE_TOKENS.reserve(token.id);
            }
        }
        for session in &self.sessions {
            ids::SESSIONS.reserve(session.id);
        }
        // Reserve disk IDs as well: new runtime objects can use their identity as a stable disk ID.
        for group in &doc.groups {
            ids::GROUPS.reserve(group.id);
            for env in group.environments.iter().flatten() {
                ids::ENVIRONMENTS.reserve(env.id);
            }
            for token in group.response_tokens.iter().flatten() {
                ids::RESPONSE_TOKENS.reserve(token.id);
            }
        }
        for request in &doc.requests {
            ids::SESSIONS.reserve(request.id);
        }
        let project = &mut self.projects[index];
        // Record identity mappings for objects created since the last load.
        for group in self.groups.iter().filter(|g| old_groups.contains(&g.id)) {
            if !project.group_ids.values().any(|id| *id == group.id) {
                project.group_ids.insert(group.id, group.id);
            }
            for env in group.environments.iter().flatten() {
                if !project.environment_ids.values().any(|id| *id == env.id) {
                    project.environment_ids.insert(env.id, env.id);
                }
            }
            for token in group.response_tokens.iter().flatten() {
                if !project.token_ids.values().any(|id| *id == token.id) {
                    project.token_ids.insert(token.id, token.id);
                }
            }
        }
        for session in self
            .sessions
            .iter()
            .filter(|s| s.group_id.is_some_and(|id| old_groups.contains(&id)))
        {
            if !project.request_ids.values().any(|id| *id == session.id) {
                project.request_ids.insert(session.id, session.id);
            }
        }
        for group in &doc.groups {
            runtime_id(&mut project.group_ids, group.id, &ids::GROUPS);
        }
        for request in &doc.requests {
            runtime_id(&mut project.request_ids, request.id, &ids::SESSIONS);
        }
        project.root_id = project.group_ids[&doc.root_id];
        let mut groups = doc.groups;
        for group in &mut groups {
            group.id = project.group_ids[&group.id];
            group.parent_id = group.parent_id.map(|id| project.group_ids[&id]);
            let previous = self.groups.iter().find(|g| g.id == group.id);
            group.collapsed = previous.is_some_and(|g| g.collapsed);
            group.active_environment_id = previous.and_then(|g| g.active_environment_id);
            restore_auth(&mut group.local_auth, secrets);
            for value in group
                .local_definitions
                .iter_mut()
                .flat_map(|d| d.values_mut())
            {
                restore_text(value, secrets);
            }
            for env in group.environments.iter_mut().flatten() {
                for value in env.values.values_mut() {
                    restore_text(value, secrets);
                }
            }
            for env in group.environments.iter_mut().flatten() {
                env.id = runtime_id(&mut project.environment_ids, env.id, &ids::ENVIRONMENTS);
            }
            if group
                .active_environment_id
                .is_some_and(|id| !group.environments.iter().flatten().any(|e| e.id == id))
            {
                group.active_environment_id = None;
            }
            for token in group.response_tokens.iter_mut().flatten() {
                token.id = runtime_id(&mut project.token_ids, token.id, &ids::RESPONSE_TOKENS);
                token.request_id = project.request_ids[&token.request_id];
            }
        }
        let mut sessions = Vec::new();
        for request in doc.requests {
            let id = project.request_ids[&request.id];
            let mut session = self
                .sessions
                .iter()
                .find(|s| s.id == id)
                .cloned()
                .unwrap_or_else(|| create_session(None));
            session.id = id;
            session.group_id = Some(project.group_ids[&request.group_id]);
            session.draft = request.draft;
            restore_auth(&mut session.draft.local_auth, secrets);
            restore_text(&mut session.draft.token, secrets);
            restore_text(&mut session.draft.username, secrets);
            restore_text(&mut session.draft.password, secrets);
            for header in &mut session.draft.headers {
                restore_text(&mut header.value, secrets);
            }
            for pair in session
                .draft
                .query
                .iter()
                .chain(&session.draft.headers)
                .chain(session.draft.form.iter().flatten())
            {
                ids::PAIRS.reserve(pair.id);
            }
            for check in session.draft.assertions.iter().flatten() {
                ids::CHECKS.reserve(check.id);
            }
            for capture in session.draft.captures.iter().flatten() {
                ids::CHECKS.reserve(capture.id);
            }
            sessions.push(session);
        }
        self.groups.retain(|g| !old_groups.contains(&g.id));
        self.groups.extend(groups);
        self.sessions
            .retain(|s| !s.group_id.is_some_and(|id| old_groups.contains(&id)));
        self.sessions.extend(sessions);
        if self.sessions.is_empty() {
            self.sessions.push(create_session(None));
        }
        self.open_ids
            .retain(|id| self.sessions.iter().any(|s| s.id == *id));
        if self
            .active_id
            .is_some_and(|id| !self.open_ids.contains(&id))
        {
            self.active_id = self.open_ids.first().copied();
        }
        self.refresh_all_stale();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{model::AuthorizationConfig, workspace_state::Workspace};

    #[test]
    fn attachments_remap_ids_and_restart_preserves_them() {
        let mut original = Workspace::new();
        let root = original.add_group("Project", None);
        let request = original.create(Some(Some(root)));
        let doc = original
            .convert_group_to_project(root, "/a".into())
            .unwrap();
        let other = original.attach_project("/b".into(), doc.clone()).unwrap();
        assert_ne!(root, other);
        let other_request = original
            .sessions
            .iter()
            .find(|s| s.group_id == Some(other))
            .unwrap()
            .id;
        assert_ne!(request, other_request);
        let mut restored = Workspace::decode(&original.encode()).unwrap();
        restored.reload_project(other, doc.clone()).unwrap();
        assert_eq!(
            restored
                .sessions
                .iter()
                .find(|s| s.group_id == Some(other))
                .unwrap()
                .id,
            other_request
        );
        assert_eq!(restored.project_document(other).unwrap(), doc);
    }

    #[test]
    fn credentials_and_captures_are_private_and_globals_are_isolated() {
        let mut ws = Workspace::new();
        let root = ws.add_group("Project", None);
        let id = ws.create(Some(Some(root)));
        ws.set_request_local_auth(
            id,
            Some(AuthorizationConfig::Bearer {
                token: "secret-value".into(),
            }),
        );
        ws.global_definitions
            .insert("global".into(), "global-value".into());
        let doc = ws.convert_group_to_project(root, "/a".into()).unwrap();
        assert!(
            !serde_json::to_string(&doc)
                .unwrap()
                .contains("secret-value")
        );
        ws.capture(
            Some(root),
            &[("capture".into(), "capture-value".into())]
                .into_iter()
                .collect(),
        );
        assert!(
            !serde_json::to_string(&ws.project_document(root).unwrap())
                .unwrap()
                .contains("capture-value")
        );
        let context = ws.token_sources(0.0).context(Some(root));
        assert!(context.workspace_definitions.is_empty());
        assert_eq!(
            context.definitions.get("capture").map(String::as_str),
            Some("capture-value")
        );
        ws.reload_project(root, doc).unwrap();
        assert_eq!(
            ws.session(id).unwrap().draft.local_auth,
            Some(AuthorizationConfig::Bearer {
                token: "secret-value".into()
            })
        );
    }

    #[test]
    fn close_reopen_after_restart_keeps_credentials() {
        let mut ws = Workspace::new();
        let root = ws.add_group("Project", None);
        let id = ws.create(Some(Some(root)));
        ws.set_request_local_auth(
            id,
            Some(AuthorizationConfig::Basic {
                username: "private-user".into(),
                password: "private-pass".into(),
            }),
        );
        ws.capture(
            Some(root),
            &[("old_capture".into(), "old-private-value".into())]
                .into_iter()
                .collect(),
        );
        let doc = ws.convert_group_to_project(root, "/a".into()).unwrap();
        assert!(
            !serde_json::to_string(&doc)
                .unwrap()
                .contains("old-private-value")
        );
        ws.close_project(root).unwrap();
        let mut restored = Workspace::decode(&ws.encode()).unwrap();
        assert_eq!(restored.attach_project("/a".into(), doc).unwrap(), root);
        assert_eq!(
            restored.session(id).unwrap().draft.local_auth,
            Some(AuthorizationConfig::Basic {
                username: "private-user".into(),
                password: "private-pass".into()
            })
        );
        assert_eq!(
            restored
                .token_sources(0.0)
                .context(Some(root))
                .definitions
                .get("old_capture")
                .map(String::as_str),
            Some("old-private-value")
        );
    }

    #[test]
    fn response_tokens_remap_and_refuse_cross_project_sources() {
        use crate::model::{CheckSource, ResponseToken};
        let mut ws = Workspace::new();
        let root = ws.add_group("Project", None);
        let source = ws.create(Some(Some(root)));
        let token = ResponseToken {
            id: crate::ids::RESPONSE_TOKENS.next(),
            name: "access".into(),
            request_id: source,
            source: CheckSource::Json,
            path: ".access".into(),
            max_age_secs: None,
        };
        ws.groups
            .iter_mut()
            .find(|g| g.id == root)
            .unwrap()
            .response_tokens = Some(vec![token]);
        let doc = ws.convert_group_to_project(root, "/a".into()).unwrap();
        let second = ws.attach_project("/b".into(), doc.clone()).unwrap();
        let second_source = ws
            .sessions
            .iter()
            .find(|s| s.group_id == Some(second))
            .unwrap()
            .id;
        assert_eq!(
            ws.group(second).unwrap().response_tokens.as_ref().unwrap()[0].request_id,
            second_source
        );
        assert_eq!(ws.project_document(second).unwrap(), doc);
        ws.groups
            .iter_mut()
            .find(|g| g.id == second)
            .unwrap()
            .response_tokens
            .as_mut()
            .unwrap()[0]
            .request_id = source;
        let context = ws.token_sources(0.0).context(Some(second));
        assert!(
            context.response_tokens["access"]
                .problem
                .as_ref()
                .unwrap()
                .contains("outside")
        );
        assert!(ws.project_document(second).is_err());
        assert!(
            ws.token_sources(0.0)
                .context(Some(second))
                .workspace_response_tokens
                .is_empty()
        );
    }

    #[test]
    fn environments_keep_private_capture_values_separate() {
        use crate::{environments::create_environment, model::EnvironmentColor};
        let mut ws = Workspace::new();
        let root = ws.add_group("Project", None);
        let a = create_environment("A", EnvironmentColor::Info);
        let b = create_environment("B", EnvironmentColor::Info);
        ws.set_group_environments(root, Some(vec![a.clone(), b.clone()]));
        ws.convert_group_to_project(root, "/a".into()).unwrap();
        ws.set_active_environment(root, Some(a.id));
        ws.capture(
            Some(root),
            &[("token".into(), "a-private".into())].into_iter().collect(),
        );
        ws.set_active_environment(root, Some(b.id));
        assert!(
            !ws.token_sources(0.0)
                .context(Some(root))
                .definitions
                .contains_key("token")
        );
        ws.capture(
            Some(root),
            &[("token".into(), "b-private".into())].into_iter().collect(),
        );
        ws.set_active_environment(root, Some(a.id));
        assert_eq!(
            ws.token_sources(0.0).context(Some(root)).definitions["token"],
            "a-private"
        );
        let doc = ws.project_document(root).unwrap();
        assert_eq!(doc.groups[0].active_environment_id, None);
        assert!(!serde_json::to_string(&doc).unwrap().contains("private"));
    }

    #[test]
    fn conversion_failure_does_not_mutate_workspace() {
        use crate::model::{CheckSource, ResponseToken};
        let mut ws = Workspace::new();
        let root = ws.add_group("Project", None);
        let source = ws.create(Some(Some(root)));
        ws.global_response_tokens.push(ResponseToken {
            id: crate::ids::RESPONSE_TOKENS.next(),
            name: "access".into(),
            request_id: source,
            source: CheckSource::Body,
            path: String::new(),
            max_age_secs: None,
        });
        let before = ws.clone();
        assert!(ws.convert_group_to_project(root, "/a".into()).is_err());
        assert_eq!(ws, before);
    }

    #[test]
    fn snapshots_protect_legacy_readers_and_capture_conversion_after_restart() {
        let mut ws = Workspace::new();
        let root = ws.add_group("Project", None);
        ws.capture(
            Some(root),
            &[("captured".into(), "private-capture".into())]
                .into_iter()
                .collect(),
        );
        let mut restored = Workspace::decode(&ws.encode()).unwrap();
        let doc = restored
            .convert_group_to_project(root, "/a".into())
            .unwrap();
        assert!(
            !serde_json::to_string(&doc)
                .unwrap()
                .contains("private-capture")
        );
        let snapshot = restored.encode();
        assert!(crate::workspace::decode_workspace(&snapshot).is_err());
        assert!(Workspace::decode(&snapshot).is_ok());
        let mut corrupt: serde_json::Value = serde_json::from_str(&snapshot).unwrap();
        corrupt["projects"][0]["groupIds"] = serde_json::json!({"1": u64::MAX});
        assert!(Workspace::decode(&corrupt.to_string()).is_err());
    }

    #[test]
    fn settings_and_undo_keep_project_boundaries() {
        use crate::workspace_state::GroupSettingsChanges;
        let mut ws = Workspace::new();
        let local = ws.add_group("Local", None);
        let root = ws.add_group("Project", None);
        let child = ws.add_group("Child", Some(root));
        let request = ws.create(Some(Some(child)));
        ws.convert_group_to_project(root, "/a".into()).unwrap();
        ws.save_group_settings(
            child,
            GroupSettingsChanges {
                parent_id: Some(Some(local)),
                ..Default::default()
            },
        );
        assert_eq!(ws.group(child).unwrap().parent_id, Some(root));
        ws.delete_group(child);
        assert_eq!(ws.session(request).unwrap().group_id, Some(root));
        assert!(ws.undo_delete());
        assert_eq!(ws.session(request).unwrap().group_id, Some(child));
        ws.delete_request(request);
        ws.close_project(root).unwrap();
        assert!(!ws.undo_delete());
        assert!(ws.session(request).is_none());
    }

    #[test]
    fn auth_token_aliases_and_headers_keep_values_private() {
        let mut ws = Workspace::new();
        let root = ws.add_group("Project", None);
        let id = ws.create(Some(Some(root)));
        ws.set_group_local_definitions(
            root,
            Some(
                [
                    ("auth_alias".into(), "{{api_key}}".into()),
                    ("api_key".into(), "secret-key".into()),
                    ("host".into(), "public-host".into()),
                ]
                .into_iter()
                .collect(),
            ),
        );
        ws.set_request_local_auth(
            id,
            Some(AuthorizationConfig::Bearer {
                token: "{{auth_alias}}".into(),
            }),
        );
        ws.session_mut(id)
            .unwrap()
            .draft
            .headers
            .push(crate::request::pair("Cookie", "secret-cookie"));
        let doc = ws.convert_group_to_project(root, "/a".into()).unwrap();
        let json = serde_json::to_string(&doc).unwrap();
        assert!(!json.contains("secret-key"));
        assert!(!json.contains("secret-cookie"));
        assert!(json.contains("public-host"));
        ws.reload_project(root, doc).unwrap();
        assert_eq!(
            ws.token_sources(0.0).context(Some(root)).definitions["api_key"],
            "secret-key"
        );
        assert_eq!(
            ws.session(id).unwrap().draft.headers.last().unwrap().value,
            "secret-cookie"
        );
    }

    #[test]
    fn explicit_transfer_preserves_scope_and_rejects_auth_changes() {
        let mut ws = Workspace::new();
        let a = ws.add_group("A", None);
        let b = ws.add_group("B", None);
        let id = ws.create(Some(Some(a)));
        ws.session_mut(id).unwrap().draft.url = "https://example.test".into();
        ws.convert_group_to_project(a, "/a".into()).unwrap();
        ws.convert_group_to_project(b, "/b".into()).unwrap();
        assert!(ws.transfer_requests(&[id], Some(b)).is_ok());
        assert_eq!(ws.session(id).unwrap().group_id, Some(b));
        ws.set_group_local_auth(
            a,
            Some(AuthorizationConfig::Bearer {
                token: "different".into(),
            }),
        );
        assert!(ws.transfer_requests(&[id], Some(a)).is_err());
        assert_eq!(ws.session(id).unwrap().group_id, Some(b));
    }

    #[test]
    fn empty_projects_and_empty_reloads_keep_snapshots_readable() {
        let mut ws = Workspace::new();
        let root = ws.add_group("Empty", None);
        let doc = ws.convert_group_to_project(root, "/empty".into()).unwrap();
        ws.close_project(root).unwrap();
        let mut restored = Workspace::decode(&ws.encode()).unwrap();
        restored
            .attach_project("/empty".into(), doc.clone())
            .unwrap();
        assert!(restored.project_document(root).unwrap().requests.is_empty());
        let blank = restored.sessions[0].id;
        restored.move_request(blank, Some(root));
        // Simulate a workspace whose sole session belongs to the project.
        restored.sessions[0].group_id = Some(root);
        restored.reload_project(root, doc).unwrap();
        assert!(Workspace::decode(&restored.encode()).is_ok());
        assert!(restored.project_document(root).unwrap().requests.is_empty());
    }

    #[test]
    fn shared_active_environment_overrides_private_base_capture() {
        use crate::{environments::create_environment, model::EnvironmentColor};
        let mut ws = Workspace::new();
        let root = ws.add_group("Project", None);
        let mut prod = create_environment("Prod", EnvironmentColor::Info);
        prod.values.insert("token".into(), "prod-value".into());
        ws.set_group_environments(root, Some(vec![prod.clone()]));
        ws.convert_group_to_project(root, "/a".into()).unwrap();
        ws.capture(
            Some(root),
            &[("token".into(), "dev-value".into())].into_iter().collect(),
        );
        ws.set_active_environment(root, Some(prod.id));
        assert_eq!(
            ws.token_sources(0.0).context(Some(root)).definitions["token"],
            "prod-value"
        );
    }

    #[test]
    fn explicit_transfer_allocates_disk_id_when_runtime_id_is_already_a_disk_key() {
        let mut ws = Workspace::new();
        let local = ws.sessions[0].id;
        ws.session_mut(local).unwrap().draft.url = "https://example.test".into();
        let mut template = Workspace::new();
        let root = template.add_group("Destination", None);
        template.create(Some(Some(root)));
        let mut doc = template
            .convert_group_to_project(root, "/template".into())
            .unwrap();
        doc.requests[0].id = local;
        let destination = ws.attach_project("/destination".into(), doc).unwrap();
        ws.transfer_requests(&[local], Some(destination)).unwrap();
        let exported = ws.project_document(destination).unwrap();
        let ids: std::collections::HashSet<u64> = exported.requests.iter().map(|r| r.id).collect();
        assert_eq!(ids.len(), 2);
        assert!(exported.validate().is_ok());
    }

    #[test]
    fn uploads_inside_project_export_as_relative_paths() {
        let mut ws = Workspace::new();
        let root = ws.add_group("Project", None);
        let id = ws.create(Some(Some(root)));
        ws.session_mut(id).unwrap().draft.body_file = Some("/project/files/body.json".into());
        let mut part = crate::request::pair("upload", "/outside/private.txt");
        part.file = Some(true);
        ws.session_mut(id).unwrap().draft.form = Some(vec![part]);
        let doc = ws
            .convert_group_to_project(root, "/project".into())
            .unwrap();
        assert_eq!(
            doc.requests[0].draft.body_file.as_deref(),
            Some("files/body.json")
        );
        assert_eq!(
            doc.requests[0].draft.form.as_ref().unwrap()[0].value,
            "/outside/private.txt"
        );
    }

    #[test]
    fn new_checkout_cannot_send_unresolved_private_auth_references() {
        let mut ws = Workspace::new();
        let root = ws.add_group("Project", None);
        let id = ws.create(Some(Some(root)));
        ws.session_mut(id).unwrap().draft.url = "https://example.test".into();
        ws.set_group_local_definitions(
            root,
            Some(
                [("api_key".into(), "private-key".into())]
                    .into_iter()
                    .collect(),
            ),
        );
        ws.set_request_local_auth(
            id,
            Some(AuthorizationConfig::Bearer {
                token: "{{api_key}}".into(),
            }),
        );
        let doc = ws
            .convert_group_to_project(root, "/original".into())
            .unwrap();
        let mut checkout = Workspace::new();
        let loaded_root = checkout.attach_project("/checkout".into(), doc).unwrap();
        let request = checkout
            .sessions
            .iter()
            .find(|s| s.group_id == Some(loaded_root))
            .unwrap();
        let context = checkout.token_sources(0.0).request_context(request);
        let result = crate::request::build_request(
            &request.draft,
            Some(crate::request::RequestContext::Resolved(&context)),
        );
        let error = result.unwrap_err();
        assert!(error.contains("Undefined token reference"));
        assert!(!error.contains("private-key"));
    }

    #[test]
    fn queued_snapshots_retain_privacy_before_disk_save_acceptance() {
        let mut ws = Workspace::new();
        let root = ws.add_group("Project", None);
        let id = ws.create(Some(Some(root)));
        ws.convert_group_to_project(root, "/project".into())
            .unwrap();
        let baseline = ws.projects[0].disk_baseline.clone();
        ws.set_group_local_definitions(
            root,
            Some(
                [("api_token".into(), "queued-secret".into())]
                    .into_iter()
                    .collect(),
            ),
        );
        ws.set_request_local_auth(
            id,
            Some(AuthorizationConfig::Bearer {
                token: "{{api_token}}".into(),
            }),
        );
        ws.remember_project_privacy();
        let encoded_a = ws.encode();
        ws.set_request_local_auth(id, Some(AuthorizationConfig::None));
        ws.remember_project_privacy();
        let encoded_b = ws.encode();
        assert_eq!(ws.projects[0].disk_baseline, baseline);
        for snapshot in [encoded_a, encoded_b] {
            let restored = Workspace::decode(&snapshot).unwrap();
            let document = restored.project_document(root).unwrap();
            assert!(
                !serde_json::to_string(&document)
                    .unwrap()
                    .contains("queued-secret")
            );
        }
    }

    #[test]
    fn invalid_response_token_edits_do_not_block_privacy_classification() {
        use crate::model::{CheckSource, ResponseToken};
        let mut ws = Workspace::new();
        let root = ws.add_group("Project", None);
        let id = ws.create(Some(Some(root)));
        ws.convert_group_to_project(root, "/project".into())
            .unwrap();
        ws.set_group_local_definitions(
            root,
            Some(
                [("api_token".into(), "unsaved-secret".into())]
                    .into_iter()
                    .collect(),
            ),
        );
        ws.set_request_local_auth(
            id,
            Some(AuthorizationConfig::Bearer {
                token: "{{api_token}}".into(),
            }),
        );
        ws.groups
            .iter_mut()
            .find(|g| g.id == root)
            .unwrap()
            .response_tokens = Some(vec![ResponseToken {
            id: crate::ids::RESPONSE_TOKENS.next(),
            name: "broken".into(),
            request_id: 0,
            source: CheckSource::Body,
            path: String::new(),
            max_age_secs: None,
        }]);
        assert!(ws.project_document(root).is_err());
        ws.remember_project_privacy();
        ws.groups
            .iter_mut()
            .find(|g| g.id == root)
            .unwrap()
            .response_tokens = None;
        ws.set_request_local_auth(id, Some(AuthorizationConfig::None));
        assert!(
            !serde_json::to_string(&ws.project_document(root).unwrap())
                .unwrap()
                .contains("unsaved-secret")
        );
    }

    #[test]
    fn removing_auth_consumers_keeps_alias_and_environment_credentials_private() {
        use crate::{environments::create_environment, model::EnvironmentColor};
        let mut ws = Workspace::new();
        let root = ws.add_group("Project", None);
        let id = ws.create(Some(Some(root)));
        ws.set_group_local_definitions(
            root,
            Some(
                [
                    ("alias".into(), "{{api_key}}".into()),
                    ("api_key".into(), "base-secret".into()),
                ]
                .into_iter()
                .collect(),
            ),
        );
        let mut environment = create_environment("Prod", EnvironmentColor::Info);
        environment
            .values
            .insert("api_key".into(), "prod-secret".into());
        ws.set_group_environments(root, Some(vec![environment]));
        ws.set_request_local_auth(
            id,
            Some(AuthorizationConfig::Bearer {
                token: "{{alias}}".into(),
            }),
        );
        ws.session_mut(id)
            .unwrap()
            .draft
            .headers
            .push(crate::request::pair("Authorization", "header-secret"));
        ws.convert_group_to_project(root, "/project".into())
            .unwrap();
        ws.set_request_local_auth(id, Some(AuthorizationConfig::None));
        ws.session_mut(id)
            .unwrap()
            .draft
            .headers
            .last_mut()
            .unwrap()
            .key = "X-Renamed".into();
        let doc = ws.project_document(root).unwrap();
        let text = serde_json::to_string(&doc).unwrap();
        for secret in ["base-secret", "prod-secret", "header-secret"] {
            assert!(!text.contains(secret), "published {secret}");
        }
        ws.reload_project(root, doc).unwrap();
        let restored = Workspace::decode(&ws.encode()).unwrap();
        let text = serde_json::to_string(&restored.project_document(root).unwrap()).unwrap();
        for secret in ["base-secret", "prod-secret", "header-secret"] {
            assert!(!text.contains(secret));
        }
        assert_eq!(
            restored.token_sources(0.0).context(Some(root)).definitions["api_key"],
            "base-secret"
        );
    }

    #[test]
    fn project_boundaries_block_moves_delete_and_busy_close() {
        let mut ws = Workspace::new();
        let local = ws.add_group("Local", None);
        let root = ws.add_group("Project", None);
        let child = ws.add_group("Child", Some(root));
        let id = ws.create(Some(Some(child)));
        ws.convert_group_to_project(root, "/a".into()).unwrap();
        ws.move_request(id, Some(local));
        ws.move_requests(&[id], None, None);
        ws.set_group_parent(child, Some(local));
        ws.move_group(root, Some(local), None);
        ws.delete_group(root);
        assert_eq!(ws.session(id).unwrap().group_id, Some(child));
        assert_eq!(ws.group(child).unwrap().parent_id, Some(root));
        assert_eq!(ws.group(root).unwrap().parent_id, None);
        ws.session_mut(id).unwrap().busy = true;
        assert!(ws.close_project(root).is_err());
        ws.session_mut(id).unwrap().busy = false;
        ws.close_project(root).unwrap();
        assert!(ws.group(root).is_none());
        assert!(ws.session(id).is_none());
        assert!(ws.group(local).is_some());
        assert!(!ws.undo_delete());
    }
}
