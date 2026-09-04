use super::*;
use crate::session::tests::make_session_and_context_with_dynamic_tools_and_rx;
use codex_protocol::dynamic_tools::DynamicToolCallOutputContentItem;
use codex_protocol::dynamic_tools::DynamicToolFunctionSpec;
use codex_protocol::dynamic_tools::DynamicToolResponse;
use codex_protocol::models::ResponseItem;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn recovered_dynamic_tool_outputs_are_idempotent_and_reject_conflicts() {
    let tool_name = "read_workspace";
    let call_id = "call-recovery";
    let (session, turn_context, _rx) =
        make_session_and_context_with_dynamic_tools_and_rx(vec![DynamicToolSpec::Function(
            DynamicToolFunctionSpec {
                name: tool_name.to_string(),
                description: "Read a workspace file.".to_string(),
                input_schema: serde_json::json!({"type": "object"}),
                defer_loading: false,
            },
        )])
        .await;
    session
        .record_conversation_items(
            turn_context.as_ref(),
            &[ResponseItem::FunctionCall {
                id: None,
                name: tool_name.to_string(),
                namespace: None,
                arguments: "{}".to_string(),
                encrypted_function_args: None,
                call_id: call_id.to_string(),
                internal_chat_message_metadata_passthrough: None,
            }],
        )
        .await;
    let recovered = RecoverDynamicToolResponse {
        call_id: call_id.to_string(),
        response: DynamicToolResponse {
            content_items: vec![DynamicToolCallOutputContentItem::InputText {
                text: "workspace-ready".to_string(),
            }],
            success: true,
        },
    };

    let outputs = prepare_dynamic_tool_outputs(
        session.as_ref(),
        &turn_context.sub_id,
        vec![recovered.clone(), recovered.clone()],
    )
    .await
    .expect("duplicate matching results should collapse");
    assert_eq!(outputs.len(), 1);
    session
        .record_conversation_items(turn_context.as_ref(), &outputs)
        .await;
    assert_eq!(
        prepare_dynamic_tool_outputs(session.as_ref(), &turn_context.sub_id, vec![recovered],)
            .await
            .expect("retrying a persisted result should be idempotent"),
        Vec::new()
    );

    let error = prepare_dynamic_tool_outputs(
        session.as_ref(),
        &turn_context.sub_id,
        vec![RecoverDynamicToolResponse {
            call_id: call_id.to_string(),
            response: DynamicToolResponse {
                content_items: vec![DynamicToolCallOutputContentItem::InputText {
                    text: "different".to_string(),
                }],
                success: true,
            },
        }],
    )
    .await
    .expect_err("a different retry must be rejected");
    assert!(matches!(
        error.details(),
        codex_protocol::error::CodexErrorDetails::InvalidRequest(_)
    ));
}
