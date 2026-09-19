//! Event dispatch and bounded workflow graph execution.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use aix_core::{AppDefinition, EventKind, Workflow, WorkflowStep, WorkflowTransition};
use aix_permission::{CapabilityResolver, HostGrant, ResolverBuildError};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    resolve_bindings, validate_app, AppValidationError, BindingError, BindingSources,
    OperationContext, OperationError, Runtime, StateStore, StateStoreError, MAX_WORKFLOW_STEPS,
};

/// Cooperative cancellation checked before each operation invocation.
#[derive(Clone, Debug, Default)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    /// Create a token in the active state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Request cancellation of dispatches using this token.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    /// Return whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

/// Host-produced event accepted by an app session.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum RuntimeEvent {
    #[serde(rename = "app.start")]
    AppStart {
        #[serde(default)]
        payload: Value,
    },
    #[serde(rename = "ui.click")]
    UiClick {
        target: String,
        #[serde(default)]
        payload: Value,
    },
    #[serde(rename = "ui.change")]
    UiChange {
        target: String,
        #[serde(default)]
        payload: Value,
    },
    #[serde(rename = "timer")]
    Timer {
        #[serde(rename = "workflowId")]
        workflow_id: String,
        #[serde(default)]
        payload: Value,
    },
}

impl RuntimeEvent {
    fn payload(&self) -> &Value {
        match self {
            Self::AppStart { payload }
            | Self::UiClick { payload, .. }
            | Self::UiChange { payload, .. }
            | Self::Timer { payload, .. } => payload,
        }
    }
}

/// Runtime safety bounds for one app session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExecutionLimits {
    /// Maximum number of active steps executed by one workflow dispatch.
    pub max_steps_per_workflow: usize,
}

impl Default for ExecutionLimits {
    fn default() -> Self {
        Self {
            max_steps_per_workflow: MAX_WORKFLOW_STEPS,
        }
    }
}

/// A timer registration that the desktop host can schedule.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TimerSubscription {
    pub workflow_id: String,
    pub interval_ms: u64,
}

/// One successfully completed step.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StepExecution {
    pub step_id: String,
    pub operation_id: String,
    pub output: Value,
}

/// Successful execution report for one matching workflow.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowExecution {
    pub workflow_id: String,
    pub steps: Vec<StepExecution>,
}

/// Successful event dispatch and resulting state snapshot.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DispatchResult {
    pub workflows: Vec<WorkflowExecution>,
    pub state: Value,
}

/// Stable workflow failure categories.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowErrorCode {
    Cancelled,
    StepLimitExceeded,
    BindingFailed,
    OperationFailed,
    PersistenceFailed,
    InvalidGraph,
}

/// Structured failure from a workflow transaction.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowError {
    pub workflow_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step_id: Option<String>,
    pub code: WorkflowErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binding_error: Option<Box<BindingError>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation_error: Option<Box<OperationError>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store_error: Option<Box<StateStoreError>>,
}

impl std::fmt::Display for WorkflowError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.workflow_id, self.message)
    }
}

impl std::error::Error for WorkflowError {}

/// Failure while constructing an app session.
#[derive(Debug)]
pub enum SessionError {
    InvalidApp(AppValidationError),
    StateStore(StateStoreError),
    InvalidState(String),
    Permissions(ResolverBuildError),
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidApp(error) => write!(formatter, "invalid app: {error}"),
            Self::StateStore(error) => write!(formatter, "state store: {error}"),
            Self::InvalidState(message) => formatter.write_str(message),
            Self::Permissions(error) => write!(formatter, "permissions: {error}"),
        }
    }
}

impl std::error::Error for SessionError {}

/// Loaded application, mutable state and workflow execution boundary.
pub struct AppSession<S: StateStore> {
    runtime: Runtime,
    app: AppDefinition,
    context: OperationContext,
    store: S,
    limits: ExecutionLimits,
}

impl<S: StateStore> AppSession<S> {
    /// Load an app with default execution limits and its persisted state.
    pub fn new(runtime: Runtime, app: AppDefinition, store: S) -> Result<Self, SessionError> {
        Self::with_limits(runtime, app, store, ExecutionLimits::default())
    }

    /// Load an app with explicit execution limits.
    pub fn with_limits(
        runtime: Runtime,
        app: AppDefinition,
        store: S,
        limits: ExecutionLimits,
    ) -> Result<Self, SessionError> {
        Self::with_grants(runtime, app, store, limits, &[])
    }

    /// Load an app with user-approved, app-bound host grants.
    pub fn with_grants(
        runtime: Runtime,
        app: AppDefinition,
        store: S,
        limits: ExecutionLimits,
        grants: &[HostGrant],
    ) -> Result<Self, SessionError> {
        validate_app(&app, runtime.operations()).map_err(SessionError::InvalidApp)?;
        if limits.max_steps_per_workflow == 0 {
            return Err(SessionError::InvalidState(
                "max_steps_per_workflow must be greater than zero".to_owned(),
            ));
        }
        let app_id = app.metadata.id.as_str();
        let resolver = CapabilityResolver::new(app_id, &app.permissions, grants)
            .map_err(SessionError::Permissions)?;
        let state = match store.load(app_id).map_err(SessionError::StateStore)? {
            Some(state) => state,
            None => {
                let state = serde_json::to_value(&app.state).map_err(|error| {
                    SessionError::InvalidState(format!(
                        "initial state could not be encoded: {error}"
                    ))
                })?;
                store
                    .save(app_id, &state)
                    .map_err(SessionError::StateStore)?;
                state
            }
        };
        let context = OperationContext::new(state)
            .map_err(|error| SessionError::InvalidState(error.message))?
            .with_permissions(Arc::new(resolver));
        Ok(Self {
            runtime,
            app,
            context,
            store,
            limits,
        })
    }

    /// Read the current app-local state snapshot.
    pub fn state(&self) -> &Value {
        self.context.state()
    }

    /// Read the validated application definition owned by this session.
    pub fn app(&self) -> &AppDefinition {
        &self.app
    }

    /// Return timer definitions for the host scheduler.
    pub fn timer_subscriptions(&self) -> Vec<TimerSubscription> {
        self.app
            .workflows
            .iter()
            .filter(|workflow| workflow.on.kind == EventKind::Timer)
            .map(|workflow| TimerSubscription {
                workflow_id: workflow.id.clone(),
                interval_ms: workflow
                    .on
                    .interval_ms
                    .expect("validated timer has interval"),
            })
            .collect()
    }

    /// Dispatch one host event to matching workflows in declaration order.
    pub fn dispatch(
        &mut self,
        event: &RuntimeEvent,
        cancellation: &CancellationToken,
    ) -> Result<DispatchResult, WorkflowError> {
        let workflows = self
            .app
            .workflows
            .iter()
            .filter(|workflow| event_matches(workflow, event))
            .cloned()
            .collect::<Vec<_>>();
        let mut executions = Vec::new();
        for workflow in workflows {
            executions.push(self.execute_workflow(&workflow, event.payload(), cancellation)?);
        }
        Ok(DispatchResult {
            workflows: executions,
            state: self.context.state().clone(),
        })
    }

    fn execute_workflow(
        &mut self,
        workflow: &Workflow,
        event: &Value,
        cancellation: &CancellationToken,
    ) -> Result<WorkflowExecution, WorkflowError> {
        let state_before = self.context.clone();
        let result = self.execute_workflow_inner(workflow, event, cancellation);
        let execution = match result {
            Ok(execution) => execution,
            Err(error) => {
                self.context = state_before;
                return Err(error);
            }
        };
        if let Err(error) = self.store.save(&self.app.metadata.id, self.context.state()) {
            self.context = state_before;
            return Err(workflow_error(
                workflow,
                None,
                WorkflowErrorCode::PersistenceFailed,
                "workflow state could not be persisted",
            )
            .with_store(error));
        }
        Ok(execution)
    }

    fn execute_workflow_inner(
        &mut self,
        workflow: &Workflow,
        event: &Value,
        cancellation: &CancellationToken,
    ) -> Result<WorkflowExecution, WorkflowError> {
        let order = topological_order(workflow).ok_or_else(|| {
            workflow_error(
                workflow,
                None,
                WorkflowErrorCode::InvalidGraph,
                "workflow graph is cyclic or references an unknown step",
            )
        })?;
        let steps = workflow
            .steps
            .iter()
            .map(|step| (step.id.as_str(), step))
            .collect::<BTreeMap<_, _>>();
        let mut active = BTreeSet::from([workflow.entry.clone()]);
        let mut outputs = BTreeMap::new();
        let mut executions = Vec::new();

        for step_id in order {
            if !active.contains(step_id) {
                continue;
            }
            if cancellation.is_cancelled() {
                return Err(workflow_error(
                    workflow,
                    Some(step_id),
                    WorkflowErrorCode::Cancelled,
                    "workflow was cancelled",
                ));
            }
            if executions.len() >= self.limits.max_steps_per_workflow {
                return Err(workflow_error(
                    workflow,
                    Some(step_id),
                    WorkflowErrorCode::StepLimitExceeded,
                    "workflow step execution limit was reached",
                ));
            }
            let step = steps.get(step_id).expect("topological step must exist");
            let input = resolve_bindings(
                &step.input,
                &BindingSources {
                    state: self.context.state(),
                    event,
                    steps: &outputs,
                },
            )
            .map_err(|error| {
                workflow_error(
                    workflow,
                    Some(step_id),
                    WorkflowErrorCode::BindingFailed,
                    "workflow input binding could not be resolved",
                )
                .with_binding(error)
            })?;
            let output = self
                .runtime
                .execute(&step.operation, &input, &mut self.context)
                .map_err(|error| {
                    workflow_error(
                        workflow,
                        Some(step_id),
                        WorkflowErrorCode::OperationFailed,
                        "workflow operation failed",
                    )
                    .with_operation(error)
                })?;
            for transition in &step.next {
                if transition_selected(transition, &output) {
                    active.insert(transition.target().to_owned());
                }
            }
            outputs.insert(step.id.clone(), output.clone());
            executions.push(StepExecution {
                step_id: step.id.clone(),
                operation_id: step.operation.clone(),
                output,
            });
        }
        Ok(WorkflowExecution {
            workflow_id: workflow.id.clone(),
            steps: executions,
        })
    }
}

impl WorkflowError {
    fn with_binding(mut self, error: BindingError) -> Self {
        self.binding_error = Some(Box::new(error));
        self
    }

    fn with_operation(mut self, error: OperationError) -> Self {
        self.operation_error = Some(Box::new(error));
        self
    }

    fn with_store(mut self, error: StateStoreError) -> Self {
        self.store_error = Some(Box::new(error));
        self
    }
}

fn workflow_error(
    workflow: &Workflow,
    step_id: Option<&str>,
    code: WorkflowErrorCode,
    message: impl Into<String>,
) -> WorkflowError {
    WorkflowError {
        workflow_id: workflow.id.clone(),
        step_id: step_id.map(str::to_owned),
        code,
        message: message.into(),
        binding_error: None,
        operation_error: None,
        store_error: None,
    }
}

fn event_matches(workflow: &Workflow, event: &RuntimeEvent) -> bool {
    match (&workflow.on.kind, event) {
        (EventKind::AppStart, RuntimeEvent::AppStart { .. }) => true,
        (EventKind::UiClick, RuntimeEvent::UiClick { target, .. })
        | (EventKind::UiChange, RuntimeEvent::UiChange { target, .. }) => {
            workflow.on.target.as_deref() == Some(target)
        }
        (EventKind::Timer, RuntimeEvent::Timer { workflow_id, .. }) => workflow.id == *workflow_id,
        _ => false,
    }
}

fn transition_selected(transition: &WorkflowTransition, output: &Value) -> bool {
    match transition {
        WorkflowTransition::Step(_) => true,
        WorkflowTransition::Conditional(conditional) => {
            let actual = if conditional.when.path.is_empty() {
                Some(output)
            } else {
                output.pointer(&conditional.when.path)
            };
            actual == Some(&conditional.when.equals)
        }
    }
}

fn topological_order(workflow: &Workflow) -> Option<Vec<&str>> {
    let steps = workflow
        .steps
        .iter()
        .map(|step| (step.id.as_str(), step))
        .collect::<BTreeMap<_, _>>();
    let mut reachable = BTreeSet::new();
    collect_reachable(&workflow.entry, &steps, &mut reachable)?;
    let mut indegree = reachable
        .iter()
        .map(|id| (*id, 0usize))
        .collect::<BTreeMap<_, _>>();
    for id in &reachable {
        for transition in &steps.get(id)?.next {
            let target = transition.target();
            if reachable.contains(target) {
                *indegree.get_mut(target)? += 1;
            }
        }
    }
    let mut emitted = BTreeSet::new();
    let mut order = Vec::with_capacity(reachable.len());
    loop {
        let next = workflow.steps.iter().find(|step| {
            reachable.contains(step.id.as_str())
                && !emitted.contains(step.id.as_str())
                && indegree.get(step.id.as_str()) == Some(&0)
        });
        let Some(step) = next else { break };
        emitted.insert(step.id.as_str());
        order.push(step.id.as_str());
        for transition in &step.next {
            if let Some(value) = indegree.get_mut(transition.target()) {
                *value = value.saturating_sub(1);
            }
        }
    }
    (order.len() == reachable.len()).then_some(order)
}

fn collect_reachable<'a>(
    id: &'a str,
    steps: &BTreeMap<&'a str, &'a WorkflowStep>,
    reachable: &mut BTreeSet<&'a str>,
) -> Option<()> {
    if !reachable.insert(id) {
        return Some(());
    }
    let step = steps.get(id)?;
    for transition in &step.next {
        collect_reachable(transition.target(), steps, reachable)?;
    }
    Some(())
}
