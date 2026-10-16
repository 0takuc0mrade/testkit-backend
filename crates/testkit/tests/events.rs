use soroban_sdk::{symbol_short, Address, IntoVal, Symbol, Val};
use soroban_testkit::core::TestEnv;
use soroban_testkit::events::{decode_topic_diagnostic, CapturedEvent, EventLog};

#[test]
fn decode_known_topic_types_succeeds() {
    let env = TestEnv::new();
    let addr = env.address();
    let sym = symbol_short!("deposit");

    let val_sym = sym.into_val(env.env());
    let val_addr = addr.clone().into_val(env.env());
    let val_amount = 1_000_i128.into_val(env.env());

    let event = CapturedEvent {
        contract: addr.clone(),
        topics: vec![val_sym, val_addr, val_amount],
        data: 42_i128.into_val(env.env()),
    };

    let decoded_sym: Symbol = event.decode_topic(env.env(), 0).expect("decode symbol");
    assert_eq!(decoded_sym, sym);

    let decoded_addr: Address = event.decode_topic(env.env(), 1).expect("decode address");
    assert_eq!(decoded_addr, addr);

    let decoded_amount: i128 = event.decode_topic(env.env(), 2).expect("decode amount");
    assert_eq!(decoded_amount, 1_000);

    let diags = event.diagnostic_topics(env.env());
    assert_eq!(diags.len(), 3);
    assert!(diags[0].contains("deposit"));
}

#[test]
fn decode_unknown_topic_value_does_not_panic_in_diagnostics() {
    let env = TestEnv::new();
    let contract = env.address();

    // Construct an arbitrary / unrecognized Val payload that cannot be decoded as Symbol
    let unknown_val = Val::from_payload(0x7fff_ffff_ffff_ffff);

    let diag_str = decode_topic_diagnostic(env.env(), &unknown_val);
    assert!(
        diag_str.contains("<unknown topic:") || diag_str.contains("val="),
        "diagnostic string should safely identify unknown topic: {diag_str}"
    );

    let event = CapturedEvent {
        contract: contract.clone(),
        topics: vec![unknown_val],
        data: 0_i128.into_val(env.env()),
    };

    // Decoding into Symbol returns a DecodeFailed error with actionable context, never panics
    let decode_res: Result<Symbol, _> = event.decode_topic(env.env(), 0);
    assert!(decode_res.is_err());
    let err = decode_res.unwrap_err();
    assert!(err.is_decode_failed());
    assert!(err.message().contains("failed to decode topic at index 0"));

    // EventLog diagnostics render the unknown topic without panicking
    let log = EventLog::from_events(env.env(), vec![event]);
    let rendered = log.diagnostics();
    assert!(rendered.contains("topics="));

    // assert_emitted failure incorporates diagnostics without panicking on unknown topic
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        log.assert_emitted(symbol_short!("missing"));
    }));
    assert!(result.is_err(), "assert_emitted should fail when topic does not match");
}

#[test]
fn decode_topic_out_of_bounds_returns_misuse() {
    let env = TestEnv::new();
    let contract = env.address();
    let event = CapturedEvent {
        contract,
        topics: vec![],
        data: 0_i128.into_val(env.env()),
    };

    let res: Result<Symbol, _> = event.decode_topic(env.env(), 0);
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert!(err.is_misuse());
    assert!(err.message().contains("out of bounds"));
}
