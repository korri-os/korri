use korrid::script::{call_plugin_operation_ts, eval_plugin_ts};

#[test]
fn private_queue_survives_indexed_prototype_poisoning() {
    let source = r#"
        export const name = 'private-queue';
        const poison = () => { throw new Error('prototype index accessed'); };
        Object.defineProperty(Array.prototype, '0', { get: poison, set: poison, configurable: true });
        Object.defineProperty(Object.prototype, '0', { get: poison, set: poison, configurable: true });
        Object.setPrototypeOf = poison;
        const cancelled = setTimeout(poison, 0);
        clearTimeout(cancelled);
        setTimeout(value => { if (value !== 'retained') throw 1; }, 0, 'retained');
    "#;
    assert_eq!(
        eval_plugin_ts(source).unwrap(),
        r#"{"name":"private-queue"}"#
    );
}

#[test]
fn interruption_releases_a_queued_module_resolver_before_disposal() {
    let source = "await new Promise(resolve => { setTimeout(resolve, 0); while (true) {} }); export const name = 'unreachable';";
    assert!(eval_plugin_ts(source).is_err());
    assert_eq!(
        eval_plugin_ts("export const name = 'fresh';").unwrap(),
        r#"{"name":"fresh"}"#
    );
}

#[test]
fn positive_delay_uses_the_real_clock_and_finishes_before_launch() {
    let started = std::time::Instant::now();
    let source = "export const name = 'clock'; let fired = false; setTimeout(() => fired = true, 20); export const handlers = {'launch.prepare': function () { return { fired }; }}";
    assert_eq!(
        call_plugin_operation_ts(source, "launch.prepare", "null").unwrap(),
        r#"{"fired":true}"#
    );
    assert!(started.elapsed() >= std::time::Duration::from_millis(20));
}

#[test]
fn two_rejected_promises_with_one_reason_keep_distinct_identities() {
    let source = "export const name = 'identities'; const reason = { toString() { throw 'must not inspect'; } }; const first = Promise.reject(reason); Promise.reject(reason); first.catch(() => {});";
    assert_eq!(
        eval_plugin_ts(source).unwrap_err(),
        "plugin has an unhandled promise rejection"
    );
}

#[test]
fn every_evaluator_installs_the_same_function_only_timers() {
    let source = "export const name = 'timers'; export const description = [typeof setTimeout, typeof clearTimeout, typeof setInterval, typeof process, typeof fetch].join('/'); export const handlers = {'launch.prepare': function () { return { timer: typeof setTimeout }; }}";
    assert_eq!(
        eval_plugin_ts(source).unwrap(),
        r#"{"description":"function/function/undefined/undefined/undefined","name":"timers"}"#
    );
    assert_eq!(
        call_plugin_operation_ts(source, "launch.prepare", "null").unwrap(),
        r#"{"timer":"function"}"#
    );
    assert!(eval_plugin_ts("export const name = 'timers'; setTimeout('1', 0);").is_err());
}

#[test]
fn initialization_drains_before_launch_but_outputs_are_captured_before_queued_mutation() {
    let source = "export const name = 'timers'; export const config = { phase: 'captured' }; let ready = false; setTimeout(() => { config.phase = 'mutated'; ready = true; }, 0); export const handlers = {'launch.prepare': function () { const output = { ready, phase: config.phase }; setTimeout(() => { output.phase = 'too late'; }, 0); return output; }}";
    assert_eq!(
        eval_plugin_ts(source).unwrap(),
        r#"{"config":{"phase":"captured"},"name":"timers"}"#
    );
    let output: serde_json::Value =
        serde_json::from_str(&call_plugin_operation_ts(source, "launch.prepare", "null").unwrap())
            .unwrap();
    assert_eq!(output, serde_json::json!({"phase":"mutated","ready":true}));
}

#[test]
fn real_timers_are_deferred_ordered_cancellable_and_drain_promise_jobs_between_callbacks() {
    let source = r#"
        export const name = 'timers';
        const events = [];
        const argument = {};
        const cancelled = setTimeout(() => { throw new Error('cancel failed'); }, 0);
        clearTimeout(String(cancelled));
        setTimeout(value => {
            if (value !== argument) throw new Error('argument identity');
            events.push('first');
            Promise.resolve().then(() => events.push('job'));
            setTimeout(() => {
                if (events.join('/') !== 'first/job/second') throw new Error('wrong order');
            }, 0);
        }, 0, argument);
        setTimeout(() => events.push('second'), 0);
        if (events.length !== 0) throw new Error('timer ran inline');
    "#;
    assert_eq!(eval_plugin_ts(source).unwrap(), r#"{"name":"timers"}"#);
}

#[test]
fn queued_errors_invalidate_captured_results_without_exposing_the_error_payload() {
    for scheduled in [
        "setTimeout(() => { throw new Error('SECRET_PAYLOAD'); }, 0);",
        "setTimeout(async () => { throw new Error('SECRET_PAYLOAD'); }, 0);",
        "Promise.reject(new Error('SECRET_PAYLOAD'));",
    ] {
        let source = format!("export const name = 'timers'; {scheduled}");
        let error = eval_plugin_ts(&source).unwrap_err();
        assert!(!error.contains("SECRET_PAYLOAD"), "{error}");
        let source = format!("export const name = 'timers'; export const handlers = {{'launch.prepare': function () {{ {scheduled} return {{ok:true}}; }}}}");
        assert!(call_plugin_operation_ts(&source, "launch.prepare", "null").is_err());
    }
}

#[test]
fn rejection_handlers_have_one_promise_job_checkpoint_not_a_later_timer() {
    assert!(eval_plugin_ts("export const name = 'handled'; const p = Promise.reject(1); Promise.resolve().then(() => p.catch(() => {}));").is_ok());
    assert!(eval_plugin_ts("export const name = 'late'; const p = Promise.reject(1); setTimeout(() => p.catch(() => {}), 0);").is_err());
    assert!(eval_plugin_ts("export const name = 'two'; const p = Promise.reject(1); Promise.reject(2); p.catch(() => {});").is_err());
}

#[test]
fn initialization_requires_fulfillment_without_timer_dependent_top_level_await() {
    assert!(eval_plugin_ts("await Promise.resolve(); export const name = 'ready';").is_ok());
    for source in [
        "await Promise.reject(1); export const name = 'no';",
        "await Promise.resolve().then(() => { throw 1; }); export const name = 'no';",
        "await new Promise(() => {}); export const name = 'no';",
        "await new Promise(resolve => setTimeout(resolve, 0)); export const name = 'no';",
    ] {
        assert!(eval_plugin_ts(source).is_err(), "accepted {source}");
        let launch = format!("{source} export const handlers = {{'launch.prepare': function () {{ return {{unexpected:true}}; }}}}");
        assert!(call_plugin_operation_ts(&launch, "launch.prepare", "null").is_err());
    }
}

#[test]
fn scheduler_does_not_use_plugin_replacements_of_critical_intrinsics() {
    let source = r#"
        export const name = 'intrinsics';
        const poison = () => { throw new Error('replaced intrinsic called'); };
        Map.prototype.set = poison;
        Map.prototype.delete = poison;
        Map.prototype.entries = poison;
        Map.prototype.clear = poison;
        Math.trunc = poison;
        Math.max = poison;
        Number.isFinite = poison;
        Reflect.apply = poison;
        Function.prototype.call = poison;
        Function.prototype.apply = poison;
        const cancelled = setTimeout(poison, 0);
        clearTimeout(cancelled);
        setTimeout(() => {}, 0);
    "#;
    assert_eq!(eval_plugin_ts(source).unwrap(), r#"{"name":"intrinsics"}"#);
}

#[test]
fn queued_work_uses_the_existing_deadline_and_cannot_survive_a_fresh_evaluation() {
    for work in [
        "setTimeout(() => {}, 10000);",
        "setTimeout(() => { while (true) {} }, 0);",
        "Promise.resolve().then(function again() { return Promise.resolve().then(again); });",
    ] {
        let source = format!("export const name = 'bounded'; {work}");
        let started = std::time::Instant::now();
        assert!(eval_plugin_ts(&source).is_err(), "accepted {work}");
        assert!(started.elapsed() < std::time::Duration::from_secs(3));
        assert_eq!(
            eval_plugin_ts("export const name = 'fresh';").unwrap(),
            r#"{"name":"fresh"}"#
        );
    }
}
