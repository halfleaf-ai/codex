//! Validates external results supplied while recovering a suspended turn.

use super::session::Session;
use codex_protocol::dynamic_tools::DynamicToolNamespaceTool;
use codex_protocol::dynamic_tools::DynamicToolSpec;
use codex_protocol::error::CodexErr;
use codex_protocol::error::Result as CodexResult;
use codex_protocol::models::FunctionCallOutputBody;
use codex_protocol::models::FunctionCallOutputPayload;
use codex_protocol::models::ResponseInputItem;
use codex_protocol::models::ResponseItem;
use codex_protocol::turn_input::RecoverDynamicToolResponse;
use std::collections::HashMap;

pub(super) async fn prepare_dynamic_tool_outputs(
    session: &Session,
    turn_id: &str,
    responses: Vec<RecoverDynamicToolResponse>,
) -> CodexResult<Vec<ResponseItem>> {
    if responses.is_empty() {
        return Ok(Vec::new());
    }

    let state = session.state.lock().await;
    let dynamic_tools = &state.session_configuration.dynamic_tools;
    let history = state.history.raw_items().collect::<Vec<_>>();
    let mut submitted_outputs = HashMap::<String, FunctionCallOutputBody>::new();
    let mut outputs = Vec::with_capacity(responses.len());

    for recovered in responses {
        let call = history.iter().rev().find_map(|item| match item {
            ResponseItem::FunctionCall {
                name,
                namespace,
                call_id,
                ..
            } if call_id == &recovered.call_id
                && item
                    .turn_id()
                    .is_none_or(|item_turn_id| item_turn_id == turn_id) =>
            {
                Some((name.as_str(), namespace.as_deref()))
            }
            _ => None,
        });
        let Some((name, namespace)) = call else {
            return Err(CodexErr::InvalidRequest(format!(
                "dynamic-tool response references unknown call id `{}` for turn `{turn_id}`",
                recovered.call_id
            )));
        };
        if !is_configured_dynamic_tool(dynamic_tools, name, namespace) {
            return Err(CodexErr::InvalidRequest(format!(
                "call id `{}` does not reference a configured dynamic tool",
                recovered.call_id
            )));
        }

        let body = FunctionCallOutputBody::ContentItems(
            recovered
                .response
                .content_items
                .into_iter()
                .map(Into::into)
                .collect(),
        );
        if let Some(previous) = submitted_outputs.get(&recovered.call_id) {
            if previous == &body {
                continue;
            }
            return Err(CodexErr::InvalidRequest(format!(
                "dynamic-tool response for call id `{}` conflicts with another submitted result",
                recovered.call_id
            )));
        }
        submitted_outputs.insert(recovered.call_id.clone(), body.clone());

        let existing = history.iter().rev().find_map(|item| match item {
            ResponseItem::FunctionCallOutput {
                call_id: Some(call_id),
                output,
                ..
            } if call_id == &recovered.call_id
                && item
                    .turn_id()
                    .is_none_or(|item_turn_id| item_turn_id == turn_id) =>
            {
                Some(&output.body)
            }
            _ => None,
        });
        if let Some(existing) = existing {
            if existing == &body {
                continue;
            }
            return Err(CodexErr::InvalidRequest(format!(
                "dynamic-tool response for call id `{}` conflicts with its persisted result",
                recovered.call_id
            )));
        }

        outputs.push(
            ResponseInputItem::FunctionCallOutput {
                call_id: recovered.call_id,
                output: FunctionCallOutputPayload {
                    body,
                    success: Some(recovered.response.success),
                },
            }
            .into(),
        );
    }
    Ok(outputs)
}

fn is_configured_dynamic_tool(
    dynamic_tools: &[DynamicToolSpec],
    name: &str,
    namespace: Option<&str>,
) -> bool {
    dynamic_tools.iter().any(|spec| match (spec, namespace) {
        (DynamicToolSpec::Function(tool), None) => tool.name == name,
        (DynamicToolSpec::Namespace(spec), Some(namespace)) if spec.name == namespace => spec
            .tools
            .iter()
            .any(|tool| matches!(tool, DynamicToolNamespaceTool::Function(tool) if tool.name == name)),
        _ => false,
    })
}

#[cfg(test)]
#[path = "turn_recovery_tests.rs"]
mod tests;
