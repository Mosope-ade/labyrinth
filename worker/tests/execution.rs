use labyrinth_engine::{artifacts, catalog, intent};
use labyrinth_worker::{
    Context, Error, Limits,
    package::{self, EngineInput},
    process,
};
use std::{path::Path, time::Duration};
fn limits() -> Limits {
    Limits {
        lease_secs: 60,
        heartbeat_secs: 15,
        deadline_secs: 120,
        cpu_secs: 60,
        memory_bytes: 1024 * 1024 * 1024,
        input_bytes: 2 * 1024 * 1024,
        output_bytes: 32 * 1024 * 1024,
        attempts: 3,
        retry_first_secs: 10,
        retry_second_secs: 60,
    }
}
fn input() -> EngineInput {
    let intent::Readiness::Ready(intent) =
        intent::parse_json(include_str!("../../tests/fixtures/intents/enterprise.json")).unwrap()
    else {
        panic!()
    };
    let yaml = include_str!("../../tests/fixtures/catalog/blueprints.yaml");
    EngineInput {
        context: Context {
            requirements: serde_json::from_slice(
                &artifacts::canonical_requirements(&intent).unwrap(),
            )
            .unwrap(),
            provenance: serde_json::from_slice(&artifacts::canonical_provenance(&intent).unwrap())
                .unwrap(),
            options: serde_json::to_value(artifacts::GenerationOptions::default()).unwrap(),
            catalog_yaml: yaml.into(),
            catalog_sha256: catalog::load_yaml(yaml).unwrap().version().sha256.clone(),
            base_revision_id: None,
            base_manifest_sha256: None,
            base_files: vec![],
            staged_prefix: "attempts/test/job/attempt/".into(),
        },
        base: None,
    }
}
#[tokio::test]
async fn real_supervised_engine_produces_valid_package_and_rejects_corruption() {
    let request = input();
    let bytes = serde_json::to_vec(&request).unwrap();
    let out = process::run(
        Path::new(env!("CARGO_BIN_EXE_labyrinth-worker")),
        Path::new("/usr/bin/prlimit"),
        bytes,
        &limits(),
    )
    .await
    .unwrap();
    let mut files = serde_json::from_slice(&out).unwrap();
    let inventory = package::validate(&files, &request.context, limits().output_bytes).unwrap();
    assert_eq!(inventory.len(), 13);
    files.get_mut("topology.json").unwrap().push(b' ');
    assert!(package::validate(&files, &request.context, limits().output_bytes).is_err());
}
#[test]
fn snapshot_roundtrip_preserves_defaults_and_rejects_provenance_mutation() {
    let request = input();
    let restored = intent::restore_snapshot(
        request.context.requirements.clone(),
        request.context.provenance.clone(),
    )
    .unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(
            &artifacts::canonical_provenance(&restored).unwrap()
        )
        .unwrap(),
        request.context.provenance
    );
    assert!(intent::restore_snapshot(request.context.requirements, serde_json::json!({})).is_err());
}
#[test]
fn limits_fail_closed() {
    let mut l = limits();
    l.heartbeat_secs = 60;
    assert!(l.validate().is_err());
    l = limits();
    l.attempts = 4;
    assert!(l.validate().is_err());
}
#[tokio::test]
async fn invalid_input_is_deterministic_and_oversize_is_rejected() {
    assert!(matches!(
        process::run(
            Path::new(env!("CARGO_BIN_EXE_labyrinth-worker")),
            Path::new("/usr/bin/prlimit"),
            b"{}".to_vec(),
            &limits()
        )
        .await,
        Err(Error::Deterministic)
    ));
    let mut l = limits();
    l.input_bytes = 1;
    assert!(matches!(
        process::run(
            Path::new("/does/not/exist"),
            Path::new("/usr/bin/prlimit"),
            vec![0, 0],
            &l
        )
        .await,
        Err(Error::Invalid)
    ));
}
#[cfg(unix)]
async fn script(body: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = std::env::temp_dir().join(format!("labyrinth-worker-test-{}", uuid::Uuid::new_v4()));
    std::fs::write(&path, body).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    path
}
#[tokio::test]
async fn hard_deadline_and_output_limit_terminate_process() {
    let path = script("#!/bin/sh\nexec /bin/sleep 30\n").await;
    let mut l = limits();
    l.deadline_secs = 1;
    l.cpu_secs = 1;
    let start = std::time::Instant::now();
    assert!(matches!(
        process::run(&path, Path::new("/usr/bin/prlimit"), vec![], &l).await,
        Err(Error::ResourceLimit)
    ));
    assert!(start.elapsed() < Duration::from_secs(4));
    std::fs::remove_file(path).unwrap();
    let path = script("#!/bin/sh\nexec /usr/bin/yes x\n").await;
    l.output_bytes = 1024;
    assert!(matches!(
        process::run(&path, Path::new("/usr/bin/prlimit"), vec![], &l).await,
        Err(Error::ResourceLimit)
    ));
    std::fs::remove_file(path).unwrap();
}

#[tokio::test]
async fn cancellation_drops_and_kills_child() {
    let pidfile =
        std::env::temp_dir().join(format!("labyrinth-child-pid-{}", uuid::Uuid::new_v4()));
    let body = format!(
        "#!/bin/sh\necho $$ > '{}'\nexec /bin/sleep 30\n",
        pidfile.display()
    );
    let path = script(&body).await;
    let script_path = path.clone();
    let task = tokio::spawn(async move {
        process::run(
            &script_path,
            Path::new("/usr/bin/prlimit"),
            vec![],
            &limits(),
        )
        .await
    });
    let mut pid = None;
    for _ in 0..100 {
        if let Ok(value) = std::fs::read_to_string(&pidfile) {
            pid = Some(value.trim().to_owned());
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let pid = pid.expect("child starts");
    task.abort();
    let _ = task.await;
    for _ in 0..100 {
        if !Path::new(&format!("/proc/{pid}")).exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        !Path::new(&format!("/proc/{pid}")).exists(),
        "cancelled child must be killed and reaped"
    );
    std::fs::remove_file(pidfile).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[tokio::test]
async fn cpu_and_address_space_limits_are_enforced() {
    let mut l = limits();
    l.cpu_secs = 1;
    l.deadline_secs = 5;
    let cpu = script("#!/bin/sh\nwhile :; do :; done\n").await;
    assert!(matches!(
        process::run(&cpu, Path::new("/usr/bin/prlimit"), vec![], &l).await,
        Err(Error::ResourceLimit)
    ));
    std::fs::remove_file(cpu).unwrap();
    let memory = script("#!/bin/sh\nexec /usr/bin/python3 -c 'x = bytearray(2147483648)'\n").await;
    assert!(matches!(
        process::run(&memory, Path::new("/usr/bin/prlimit"), vec![], &l).await,
        Err(Error::ResourceLimit)
    ));
    std::fs::remove_file(memory).unwrap();
}

#[tokio::test]
async fn cooperative_search_exhaustion_remains_inconclusive_across_child_boundary() {
    let mut request = input();
    request.context.options["selection_search_states"] = serde_json::json!(1);
    assert!(matches!(
        package::generate(serde_json::from_slice(&serde_json::to_vec(&request).unwrap()).unwrap()),
        Err(Error::Engine(ref report)) if report.outcome == labyrinth_worker::evidence::Outcome::Inconclusive
    ));
    assert!(matches!(
        process::run(
            Path::new(env!("CARGO_BIN_EXE_labyrinth-worker")),
            Path::new("/usr/bin/prlimit"),
            serde_json::to_vec(&request).unwrap(),
            &limits()
        )
        .await,
        Err(Error::Engine(ref report)) if report.outcome == labyrinth_worker::evidence::Outcome::Inconclusive
    ));
}

#[tokio::test]
async fn engine_failure_evidence_crosses_supervised_boundary_without_exception_text() {
    let mut request = input();
    request.context.options["selection_search_states"] = serde_json::json!(1);
    let error = process::run(
        Path::new(env!("CARGO_BIN_EXE_labyrinth-worker")),
        Path::new("/usr/bin/prlimit"),
        serde_json::to_vec(&request).unwrap(),
        &limits(),
    )
    .await
    .unwrap_err();
    let Error::Engine(evidence) = error else {
        panic!("structured evidence required")
    };
    assert_eq!(
        evidence.outcome,
        labyrinth_worker::evidence::Outcome::Inconclusive
    );
    assert!(evidence.error.is_some());
    assert!(!format!("{}", Error::Engine(evidence)).contains("planning"));
}
