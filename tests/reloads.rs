use chroma::adapters::reload;

#[test]
fn reload_success_failure_missing_and_timeout_are_bounded() {
    assert!(reload::run(&[], 100).is_ok());
    assert!(reload::run(&["/usr/bin/true".into()], 1000).is_ok());
    assert!(reload::run(&["/usr/bin/false".into()], 1000).is_err());
    assert!(reload::run(&["/definitely/not/installed".into()], 100).is_err());
    let start = std::time::Instant::now();
    let error = reload::run(&["/usr/bin/sleep".into(), "10".into()], 20).unwrap_err();
    assert!(error.to_string().contains("timed out"));
    assert!(start.elapsed() < std::time::Duration::from_secs(3));
}

#[test]
fn arguments_are_never_interpreted_by_a_shell() {
    let temp = tempfile::tempdir().unwrap();
    let marker = temp.path().join("should-not-exist");
    let argument = format!("$(touch {}); touch {}", marker.display(), marker.display());
    assert!(reload::run(&["/usr/bin/printf".into(), argument], 1000).is_ok());
    assert!(!marker.exists());
}
