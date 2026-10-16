//! Focused coverage for the definition of a "known actor" in `AuthMatrix`
//! reports: the known-actor set is the de-duplicated union of every entry
//! point's `allowed` list, and the failure report states that definition.

use soroban_testkit::auth::AuthMatrix;
use soroban_testkit::core::TestEnv;

#[test]
fn known_actors_is_the_deduped_union_in_first_seen_order() {
    let env = TestEnv::new();
    let alice = env.address();
    let bob = env.address();
    let carol = env.address();

    let matrix = AuthMatrix::new(&env)
        .entry_point("a", &[alice.clone(), bob.clone()], |_| Ok(()))
        .entry_point("b", &[bob.clone(), carol.clone()], |_| Ok(()));

    // `bob` is named on both entry points but appears once.
    assert_eq!(matrix.known_actors(), vec![alice, bob, carol]);
}

#[test]
fn addresses_never_listed_are_not_known_actors() {
    let env = TestEnv::new();
    let listed = env.address();
    let never_listed = env.address();

    let matrix =
        AuthMatrix::new(&env).entry_point("only", std::slice::from_ref(&listed), |_| Ok(()));

    let known = matrix.known_actors();
    assert_eq!(known, vec![listed]);
    assert!(!known.contains(&never_listed));
}

#[test]
fn report_states_the_known_actor_definition_and_names_the_offender() {
    let env = TestEnv::new();
    let owner = env.address();
    let attacker = env.address();

    // `open` allows both callers, so both are known actors; `guarded` is
    // buggy and lets everyone through, so the known actor `attacker` wrongly
    // succeeds there and must be flagged.
    let matrix = AuthMatrix::new(&env)
        .entry_point("open", &[owner.clone(), attacker.clone()], |_| Ok(()))
        .entry_point("guarded", std::slice::from_ref(&owner), |_| Ok(()));

    let err = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| matrix.assert_enforced()))
        .expect_err("guarded allows everyone, so the attacker cell must be caught");
    let message = err
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| err.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_default();

    assert!(
        message.contains("known actors = union of all entry points' allowed lists"),
        "the report must state the known-actor definition: {message}"
    );
    assert!(
        message.contains(&format!("{attacker:?}")),
        "the report must name the offending known actor: {message}"
    );
    assert!(
        message.contains("guarded"),
        "the report must name the entry point that let the actor through: {message}"
    );
}
