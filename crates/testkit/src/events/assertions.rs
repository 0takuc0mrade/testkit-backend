use soroban_sdk::{Env, IntoVal, TryFromVal, Val};

use crate::core::TestkitError;
use crate::events::capture::EventLog;

impl EventLog {
    /// Assert that at least one captured event has the given topic.
    ///
    /// # Panics
    ///
    /// Panics with a [`TestkitError::AssertionFailed`] listing every
    /// captured event (contract, topics, data) if none match — the usual
    /// cause is a typo'd topic symbol, and the user needs to see what
    /// *was* emitted.
    ///
    /// # Example
    ///
    /// ```should_panic
    /// use soroban_testkit::core::TestEnv;
    /// use soroban_sdk::symbol_short;
    ///
    /// let env = TestEnv::new();
    /// env.events().assert_emitted(symbol_short!("transfer"));
    /// ```
    pub fn assert_emitted<T: IntoVal<Env, Val>>(&self, topic: T) {
        if self.with_topic(topic).is_empty() {
            panic!(
                "{}",
                TestkitError::AssertionFailed(format!(
                    "expected an event with the given topic, but none was found\n{}",
                    self.render()
                ))
            );
        }
    }

    /// Assert that exactly `n` captured events have the given topic.
    ///
    /// # Panics
    ///
    /// Panics with a [`TestkitError::AssertionFailed`] showing the expected
    /// and actual counts plus the full captured log if they differ.
    ///
    /// # Example
    ///
    /// ```
    /// use soroban_testkit::core::TestEnv;
    /// use soroban_sdk::symbol_short;
    ///
    /// let env = TestEnv::new();
    /// env.events().assert_emitted_times(symbol_short!("transfer"), 0);
    /// ```
    pub fn assert_emitted_times<T: IntoVal<Env, Val>>(&self, topic: T, n: usize) {
        let matched = self.with_topic(topic);
        if matched.len() != n {
            panic!(
                "{}",
                TestkitError::AssertionFailed(format!(
                    "expected {n} event(s) with the given topic, found {}\n{}",
                    matched.len(),
                    self.render()
                ))
            );
        }
    }

    /// Assert that no captured event has the given topic.
    ///
    /// # Panics
    ///
    /// Panics with a [`TestkitError::AssertionFailed`] listing the matching
    /// events if any are found.
    ///
    /// # Example
    ///
    /// ```
    /// use soroban_testkit::core::TestEnv;
    /// use soroban_sdk::symbol_short;
    ///
    /// let env = TestEnv::new();
    /// env.events().assert_none(symbol_short!("transfer"));
    /// ```
    pub fn assert_none<T: IntoVal<Env, Val>>(&self, topic: T) {
        let matched = self.with_topic(topic);
        if !matched.is_empty() {
            panic!(
                "{}",
                TestkitError::AssertionFailed(format!(
                    "expected no events with the given topic, found {}\n{}",
                    matched.len(),
                    matched.render()
                ))
            );
        }
    }

    /// Decode the data payload of the single event in this log.
    ///
    /// # Panics
    ///
    /// Panics with a [`TestkitError::AssertionFailed`] if this log does not
    /// contain exactly one event (filter first, e.g. with
    /// [`EventLog::with_topic`]), or a [`TestkitError::DecodeFailed`] if
    /// the payload cannot be converted to `D`.
    ///
    /// # Example
    ///
    /// ```
    /// use soroban_testkit::core::TestEnv;
    /// use soroban_sdk::symbol_short;
    ///
    /// let env = TestEnv::new();
    /// let log = env.events().with_topic(symbol_short!("no_match"));
    /// assert!(log.is_empty());
    /// ```
    pub fn decode_one<D: TryFromVal<Env, Val>>(&self) -> D {
        match self.events.as_slice() {
            [only] => D::try_from_val(&self.env, &only.data).unwrap_or_else(|_| {
                panic!(
                    "{}",
                    TestkitError::DecodeFailed(format!(
                        "failed to decode event data as the requested type\n{}",
                        self.render()
                    ))
                )
            }),
            [] => panic!(
                "{}",
                TestkitError::AssertionFailed(format!(
                    "decode_one requires exactly one event, but this log is empty\n{}",
                    self.render()
                ))
            ),
            _ => panic!(
                "{}",
                TestkitError::AssertionFailed(format!(
                    "decode_one requires exactly one event, but this log has {}\n{}",
                    self.events.len(),
                    self.render()
                ))
            ),
        }
    }

    /// Render every captured event (contract, topics, data) for inclusion
    /// in an assertion failure message.
    pub(crate) fn render(&self) -> String {
        if self.events.is_empty() {
            return "captured events: (none)".to_string();
        }
        let mut out = format!("captured events ({}):\n", self.events.len());
        for (i, event) in self.events.iter().enumerate() {
            let rendered_topics: Vec<String> = event
                .topics
                .iter()
                .map(|t| crate::events::capture::decode_topic_diagnostic(&self.env, t))
                .collect();
            let rendered_data =
                crate::events::capture::decode_val_diagnostic(&self.env, &event.data);
            out.push_str(&format!(
                "  [{i}] contract={:?} topics={:?} data={}\n",
                event.contract, rendered_topics, rendered_data
            ));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use crate::core::TestEnv;
    use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env, Symbol};

    #[contract]
    struct Emitter;

    #[contractimpl]
    impl Emitter {
        pub fn emit(env: Env, topic: Symbol, value: i128) {
            #[allow(deprecated)]
            env.events().publish((topic,), value);
        }
    }

    #[test]
    fn assert_emitted_passes_when_present() {
        let env = TestEnv::new();
        let id = env.env().register(Emitter, ());
        let client = EmitterClient::new(env.env(), &id);
        client.emit(&symbol_short!("transfer"), &100);

        env.events().assert_emitted(symbol_short!("transfer"));
    }

    #[test]
    #[should_panic(expected = "expected an event with the given topic, but none was found")]
    fn assert_emitted_panics_with_full_log_when_absent() {
        let env = TestEnv::new();
        let id = env.env().register(Emitter, ());
        let client = EmitterClient::new(env.env(), &id);
        client.emit(&symbol_short!("mint"), &1);

        env.events().assert_emitted(symbol_short!("transfer"));
    }

    #[test]
    fn assert_emitted_times_counts_matching_events() {
        let env = TestEnv::new();
        let id = env.env().register(Emitter, ());
        let client = EmitterClient::new(env.env(), &id);
        client.emit(&symbol_short!("transfer"), &1);

        env.events()
            .assert_emitted_times(symbol_short!("transfer"), 1);
    }

    #[test]
    #[should_panic(expected = "expected 2 event(s) with the given topic, found 1")]
    fn assert_emitted_times_panics_on_mismatch() {
        let env = TestEnv::new();
        let id = env.env().register(Emitter, ());
        let client = EmitterClient::new(env.env(), &id);
        client.emit(&symbol_short!("transfer"), &1);

        env.events()
            .assert_emitted_times(symbol_short!("transfer"), 2);
    }

    #[test]
    fn assert_none_passes_when_absent() {
        let env = TestEnv::new();
        let id = env.env().register(Emitter, ());
        let client = EmitterClient::new(env.env(), &id);
        client.emit(&symbol_short!("mint"), &1);

        env.events().assert_none(symbol_short!("transfer"));
    }

    #[test]
    #[should_panic(expected = "expected no events with the given topic, found 1")]
    fn assert_none_panics_when_present() {
        let env = TestEnv::new();
        let id = env.env().register(Emitter, ());
        let client = EmitterClient::new(env.env(), &id);
        client.emit(&symbol_short!("transfer"), &1);

        env.events().assert_none(symbol_short!("transfer"));
    }

    #[test]
    fn decode_one_decodes_the_single_matching_events_data() {
        let env = TestEnv::new();
        let id = env.env().register(Emitter, ());
        let client = EmitterClient::new(env.env(), &id);
        client.emit(&symbol_short!("mint"), &555);

        let value: i128 = env.events().with_topic(symbol_short!("mint")).decode_one();
        assert_eq!(value, 555);
    }

    #[test]
    #[should_panic(expected = "decode_one requires exactly one event, but this log is empty")]
    fn decode_one_panics_on_empty_log() {
        let env = TestEnv::new();
        let _: i128 = env.events().decode_one();
    }

    // A router that calls into three sub-contracts from within a single
    // top-level invocation, so their events all land in one EventLog (the
    // underlying SDK resets the captured log at each top-level call — see
    // EventLog's doc comment on scope).
    #[contract]
    struct Router;

    #[contractimpl]
    impl Router {
        pub fn fan_out(env: Env, a: Address, b: Address, c: Address) {
            EmitterClient::new(&env, &a).emit(&symbol_short!("transfer"), &1);
            EmitterClient::new(&env, &b).emit(&symbol_short!("transfer"), &2);
            EmitterClient::new(&env, &c).emit(&symbol_short!("mint"), &3);
        }
    }

    #[test]
    fn chained_filters_compose_across_three_contracts_and_overlapping_topics() {
        let env = TestEnv::new();
        let a = env.env().register(Emitter, ());
        let b = env.env().register(Emitter, ());
        let c = env.env().register(Emitter, ());
        let router = env.env().register(Router, ());
        let router_client = RouterClient::new(env.env(), &router);

        router_client.fan_out(&a, &b, &c);

        let log = env.events();
        assert_eq!(log.len(), 3);
        assert_eq!(log.from(&a).len(), 1);
        assert_eq!(log.from(&b).len(), 1);
        assert_eq!(log.from(&c).len(), 1);
        assert_eq!(log.with_topic(symbol_short!("transfer")).len(), 2);
        assert_eq!(log.with_topic(symbol_short!("mint")).len(), 1);
        assert_eq!(log.from(&a).with_topic(symbol_short!("transfer")).len(), 1);
        assert_eq!(log.from(&c).with_topic(symbol_short!("transfer")).len(), 0);
        assert_eq!(log.from(&c).with_topic(symbol_short!("mint")).len(), 1);
    }
}
