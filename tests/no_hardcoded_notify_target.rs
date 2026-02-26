#[test]
fn notify_rs_should_not_hardcode_telegram_chat_id() {
    // `cam watch --openclaw` should not bake a single Telegram user id into the binary.
    // We want the target to come from OpenClaw config detection instead.
    let src = include_str!("../src/notification/watcher.rs");
    assert!(
        !src.contains("1440537501"),
        "src/notification/watcher.rs still contains a hardcoded Telegram chat id"
    );
}
