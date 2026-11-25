//! Side questions (feature `side_question`, additive in protocol 5): names on
//! the wire, the question never printed, and a device's right to ask.

use crate::method::{Method, SideAskParams};
use crate::remote::{self, Needs};
use crate::rpc::{Call, FEATURES, ResponseBody, ServerMessage, SideAnswer, SideAsked, feature, quotes_no_params, reject_unparsed};
use aurelius_domain::ConversationId;
use aurelius_domain::remote::Capability;

const QUESTION: &str = "what is FAKE-SECRET doing";

fn ask() -> Method {
    Method::ConversationSideAsk(SideAskParams { conversation_id: ConversationId::new(), question: QUESTION.into() })
}

#[test]
fn the_server_says_it_has_side_questions() {
    assert!(FEATURES.contains(&feature::SIDE_QUESTION));
    assert_eq!(feature::SIDE_QUESTION, "side_question");
}

#[test]
fn the_methods_and_the_answer_have_their_names_on_the_wire() {
    let asked = ask();
    let json = serde_json::to_value(&asked).unwrap();
    assert_eq!(json["method"], "conversation.side_ask");
    assert_eq!(json["params"]["question"], QUESTION);
    assert_eq!(serde_json::from_value::<Method>(json).unwrap(), asked);
    let cancel = serde_json::to_value(Method::ConversationSideCancel { side_id: "s".into() }).unwrap();
    assert_eq!(cancel, serde_json::json!({ "method": "conversation.side_cancel", "params": { "side_id": "s" } }));
    assert!(Method::knows("conversation.side_ask") && Method::knows("conversation.side_cancel"));
    let body = serde_json::to_value(ResponseBody::SideAsked(SideAsked { side_id: "s".into() })).unwrap();
    assert_eq!(body["type"], "side_asked");
    let pushed = ServerMessage::SideAnswer(SideAnswer { side_id: "s".into(), text: "Working on it".into(), done: false, error: None });
    let json = serde_json::to_value(&pushed).unwrap();
    assert!(json.get("error").is_none(), "no error is left out: {json}");
    assert_eq!(serde_json::from_value::<ServerMessage>(json).unwrap(), pushed);
}

#[test]
fn the_question_is_never_printed() {
    assert!(!format!("{:?}", ask()).contains("FAKE-SECRET"));
    assert!(quotes_no_params("conversation.side_ask"));
    let bad = serde_json::json!({ "request_id": 7, "method": "conversation.side_ask", "params": { "question": QUESTION } });
    let rejected = reject_unparsed(Some(&bad), &"missing field conversation_id; FAKE-SECRET");
    assert!(!format!("{rejected:?}").contains("FAKE-SECRET"));
}

#[test]
fn a_device_needs_task_control_to_ask_or_stop() {
    let needs = |method: Method| remote::required(&Call::Read(method));
    assert_eq!(needs(ask()), Needs::Cap(Capability::TaskControl));
    assert_eq!(needs(Method::ConversationSideCancel { side_id: "s".into() }), Needs::Cap(Capability::TaskControl));
    assert!(!remote::grants(&[Capability::Read, Capability::Chat], Needs::Cap(Capability::TaskControl)));
}
