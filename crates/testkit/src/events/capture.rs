use soroban_sdk::testutils::Events as _;
use soroban_sdk::{xdr, Address, Env, IntoVal, TryFromVal, Val};

use crate::core::TestEnv;

/// A single contract event, captured from the environment.
///
/// `topics` and `data` are the **raw** `Val`s exactly as published — the same
/// values `soroban_sdk` captured for the event — held as `Val` so they can be
/// compared and decoded on demand (for example with
/// [`EventLog::decode_one`]). They are not eagerly converted into other types.
///
/// [`CapturedEvent::render`] prints those raw values in their decoded form
/// (`contract=... topics=[Symbol(mint)] data=I128(1)`), which is exactly what
/// assertion-failure diagnostics include, so the diagnostic a failed
/// assertion shows is always aligned with the event that was captured.
#[derive(Debug, Clone)]
pub struct CapturedEvent {
    /// The contract that published the event.
    pub contract: Address,
    /// The event's topics, in publish order.
    pub topics: Vec<Val>,
    /// The event's data payload.
    pub data: Val,
}

impl CapturedEvent {
    /// Render this captured event as a single diagnostic line.
    ///
    /// The topics and data are shown in their decoded form (the `Debug` of the
    /// raw captured `Val`s), so a failure message built from this method lines
    /// up with the values the event actually carried.
    ///
    /// # Example
    ///
    /// ```
    /// use soroban_sdk::IntoVal;
    /// use soroban_testkit::core::TestEnv;
    /// use soroban_testkit::events::CapturedEvent;
    ///
    /// let env = TestEnv::new();
    /// let event = CapturedEvent {
    ///     contract: env.address(),
    ///     topics: Vec::new(),
    ///     data: 0_i128.into_val(env.env()),
    /// };
    /// assert!(event.render().contains("data=I128(0)"));
    /// ```
    pub fn render(&self) -> String {
        format!(
            "contract={:?} topics={:?} data={:?}",
            self.contract, self.topics, self.data
        )
    }
}

/// A filterable, assertable log of captured contract events.
///
/// Obtained from [`TestEnv::events`] or [`TestEnv::events_during`]. Filters
/// return a new `EventLog`, so they chain:
/// `env.events().from(&contract).with_topic(symbol_short!("transfer"))`.
///
/// # A note on scope and nested invocations
///
/// `soroban_sdk::testutils::Events::all` — which this is built on — returns
/// events from the most recent top-level contract invocation, not a full
/// history accumulated since the `TestEnv` was created (verified against
/// soroban-sdk 27.0.6; its own doc comment says as much). Call
/// [`TestEnv::events`] or [`TestEnv::events_during`] right after the
/// invocation you want to inspect.
///
/// This is what makes the log cover **nested** calls correctly:
///
/// - Events published by a sub-contract call made from inside the invoked
///   entry point are included, no matter how deep the nesting goes. Each event
///   is attributed to the contract that actually published it (its
///   [`CapturedEvent::contract`]), not to the top-level entry point.
/// - All nested calls made during one top-level invocation accumulate into
///   the same log; a depth-2 call is not a separate capture window.
/// - The window is bounded by the **top-level** invocation: starting the next
///   top-level call (for example the next client method call from the test)
///   resets the captured log, so events from the previous top-level call are
///   no longer returned.
///
/// In short: one log per top-level call, spanning every nested sub-call of
/// that call.
#[derive(Clone)]
pub struct EventLog {
    pub(crate) env: Env,
    pub(crate) events: Vec<CapturedEvent>,
}

impl EventLog {
    pub(crate) fn capture(env: &Env) -> Self {
        let raw = env.events().all();
        let events = raw
            .events()
            .iter()
            .filter_map(|event| {
                let contract_id = event.contract_id.clone()?;
                let xdr::ContractEventBody::V0(body) = &event.body;
                let contract =
                    Address::try_from_val(env, &xdr::ScAddress::Contract(contract_id)).ok()?;
                let topics = body
                    .topics
                    .iter()
                    .filter_map(|topic| Val::try_from_val(env, topic).ok())
                    .collect();
                let data = Val::try_from_val(env, &body.data).ok()?;
                Some(CapturedEvent {
                    contract,
                    topics,
                    data,
                })
            })
            .collect();
        Self {
            env: env.clone(),
            events,
        }
    }

    /// The number of captured events.
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Whether no events were captured.
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Keep only events published by `contract`.
    ///
    /// # Example
    ///
    /// ```
    /// use soroban_testkit::core::TestEnv;
    ///
    /// let env = TestEnv::new();
    /// let contract = env.address();
    /// let log = env.events().from(&contract);
    /// assert!(log.is_empty());
    /// ```
    pub fn from(&self, contract: &Address) -> EventLog {
        EventLog {
            env: self.env.clone(),
            events: self
                .events
                .iter()
                .filter(|e| &e.contract == contract)
                .cloned()
                .collect(),
        }
    }

    /// Keep only events with a topic equal to `topic`, once converted into
    /// this environment's `Val` representation.
    ///
    /// # Example
    ///
    /// ```
    /// use soroban_testkit::core::TestEnv;
    /// use soroban_sdk::symbol_short;
    ///
    /// let env = TestEnv::new();
    /// let log = env.events().with_topic(symbol_short!("transfer"));
    /// assert!(log.is_empty());
    /// ```
    pub fn with_topic<T: IntoVal<Env, Val>>(&self, topic: T) -> EventLog {
        let query = topic.into_val(&self.env);
        EventLog {
            env: self.env.clone(),
            events: self
                .events
                .iter()
                .filter(|e| e.topics.iter().any(|t| vals_equal(&self.env, t, &query)))
                .cloned()
                .collect(),
        }
    }
}

/// Compares two `Val`s for structural equality by round-tripping both
/// through XDR (`ScVal`), which compares by value rather than by object
/// handle identity. A conversion failure is treated as "not equal" rather
/// than propagated, since this is used only for filtering.
pub(crate) fn vals_equal(env: &Env, a: &Val, b: &Val) -> bool {
    let a = xdr::ScVal::try_from_val(env, a);
    let b = xdr::ScVal::try_from_val(env, b);
    matches!((a, b), (Ok(a), Ok(b)) if a == b)
}

impl TestEnv {
    /// The events captured so far — see [`EventLog`]'s note on scope: this
    /// reflects the most recent top-level contract invocation, per the
    /// underlying SDK's `testutils::Events::all`.
    ///
    /// # Example
    ///
    /// ```
    /// use soroban_testkit::core::TestEnv;
    ///
    /// let env = TestEnv::new();
    /// assert!(env.events().is_empty());
    /// ```
    pub fn events(&self) -> EventLog {
        EventLog::capture(self.env())
    }

    /// Run `f`, then return its result alongside the events published
    /// while it ran.
    ///
    /// # Example
    ///
    /// ```
    /// use soroban_testkit::core::TestEnv;
    ///
    /// let env = TestEnv::new();
    /// let (doubled, log) = env.events_during(|| 21 * 2);
    /// assert_eq!(doubled, 42);
    /// assert!(log.is_empty());
    /// ```
    pub fn events_during<T>(&self, f: impl FnOnce() -> T) -> (T, EventLog) {
        let result = f();
        (result, self.events())
    }
}

#[cfg(test)]
mod capture_tests {
    use super::*;

    #[test]
    fn empty_log_reports_empty() {
        let env = TestEnv::new();
        let log = env.events();
        assert!(log.is_empty());
        assert_eq!(log.len(), 0);
    }

    #[test]
    fn from_and_with_topic_return_new_empty_logs_on_no_match() {
        let env = TestEnv::new();
        let contract = env.address();
        assert!(env.events().from(&contract).is_empty());
        assert!(env
            .events()
            .with_topic(soroban_sdk::symbol_short!("nope"))
            .is_empty());
    }
}
