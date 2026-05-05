use serde::Serialize;

#[derive(Serialize)]
struct Snapshot {
    adapter_trait_methods: &'static [&'static str],
    platform_entry_fields: &'static [&'static str],
    builtin_platform_values: &'static [&'static str],
    smoke_status: hermes_gateway::AdapterStatus,
    session_guard_trace: hermes_gateway::GuardTrace,
    gateway_command_routes:
        std::collections::BTreeMap<String, Option<hermes_gateway::GatewayCommandRoute>>,
    streaming_delivery: hermes_gateway::StreamingDeliverySnapshot,
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let smoke_status = hermes_gateway::smoke_adapter_roundtrip()
        .await
        .expect("in-memory adapter smoke succeeds");
    let snapshot = Snapshot {
        adapter_trait_methods: hermes_gateway::adapter_trait_methods(),
        platform_entry_fields: hermes_gateway::platform_entry_fields(),
        builtin_platform_values: hermes_gateway::builtin_platform_values(),
        smoke_status,
        session_guard_trace: hermes_gateway::smoke_session_guard_trace(),
        gateway_command_routes: hermes_gateway::gateway_command_route_samples(),
        streaming_delivery: hermes_gateway::streaming_delivery_snapshot(),
    };
    println!(
        "{}",
        serde_json::to_string(&snapshot).expect("adapter snapshot serializes")
    );
}
