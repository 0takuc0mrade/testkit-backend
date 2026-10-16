//! Focused coverage for event capture: decoded diagnostics must line up with
//! the raw captured values, and the captured scope must span nested
//! sub-contract calls while resetting at each top-level invocation.

use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env, Symbol};
use soroban_testkit::core::TestEnv;

#[contract]
struct Emitter;

#[contractimpl]
impl Emitter {
    pub fn emit(env: Env, topic: Symbol, value: i128) {
        #[allow(deprecated)]
        env.events().publish((topic,), value);
    }
}

// A middle contract that relays to an emitter at depth 2
// (test -> Middle -> Emitter).
#[contract]
struct Middle;

#[contractimpl]
impl Middle {
    pub fn relay(env: Env, target: Address, value: i128) {
        EmitterClient::new(&env, &target).emit(&symbol_short!("transfer"), &value);
    }
}

#[test]
fn captured_data_decodes_to_the_published_value() {
    let env = TestEnv::new();
    let id = env.env().register(Emitter, ());
    EmitterClient::new(env.env(), &id).emit(&symbol_short!("mint"), &7);

    let value: i128 = env.events().with_topic(symbol_short!("mint")).decode_one();
    assert_eq!(value, 7);
}

#[test]
fn failure_diagnostic_shows_the_decoded_captured_values() {
    let env = TestEnv::new();
    let id = env.env().register(Emitter, ());
    EmitterClient::new(env.env(), &id).emit(&symbol_short!("mint"), &7);

    let err = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        env.events().assert_emitted(symbol_short!("transfer"));
    }))
    .expect_err("no transfer event was emitted, so the assertion must fail");

    let message = err
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| err.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_default();
    assert!(message.contains("Symbol(mint)"), "{message}");
    assert!(message.contains("I128(7)"), "{message}");
}

#[test]
fn nested_sub_call_events_are_captured_and_attributed_to_the_publisher() {
    let env = TestEnv::new();
    let emitter = env.env().register(Emitter, ());
    let middle = env.env().register(Middle, ());
    MiddleClient::new(env.env(), &middle).relay(&emitter, &42);

    let log = env.events();
    assert_eq!(log.len(), 1, "a depth-2 nested event must be captured");
    assert_eq!(log.from(&emitter).len(), 1);
    assert_eq!(log.from(&middle).len(), 0);
    let value: i128 = log.with_topic(symbol_short!("transfer")).decode_one();
    assert_eq!(value, 42);
}

#[test]
fn each_top_level_call_resets_the_captured_log() {
    let env = TestEnv::new();
    let id = env.env().register(Emitter, ());
    let client = EmitterClient::new(env.env(), &id);

    client.emit(&symbol_short!("first"), &1);
    assert_eq!(env.events().with_topic(symbol_short!("first")).len(), 1);

    client.emit(&symbol_short!("second"), &2);
    let log = env.events();
    assert_eq!(
        log.len(),
        1,
        "events from the previous top-level call must not carry over"
    );
    assert_eq!(log.with_topic(symbol_short!("second")).len(), 1);
    assert_eq!(log.with_topic(symbol_short!("first")).len(), 0);
}
