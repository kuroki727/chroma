use chroma::{
    config::{Adapter, Config, Paths},
    engine::{self, Entry, Pending, State, Transaction, apply, backup, rollback},
    palette::presets,
    safe_fs::{Dir, Snapshot},
};
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::PathBuf,
};
use tempfile::{TempDir, tempdir};

struct Fixture {
    _root: TempDir,
    paths: Paths,
    config: Config,
}

impl Fixture {
    fn new() -> Self {
        let root = tempdir().unwrap();
        let paths = Paths::discover(Some(root.path().join("config")), None).unwrap();
        Dir::open(&paths.config.join("generated"), true).unwrap();
        Dir::open(&paths.config.join("templates"), true).unwrap();
        fs::write(
            paths.config.join("templates/test"),
            "{{ background }} {{primary.rgb}}",
        )
        .unwrap();
        let mut config = Config::default();
        config.adapters.insert(
            "test".into(),
            Adapter {
                enabled: true,
                template: Some("templates/test".into()),
                output: Some("generated/test".into()),
                ..Adapter::default()
            },
        );
        Self {
            _root: root,
            paths,
            config,
        }
    }
    fn output(&self) -> PathBuf {
        self.paths.config.join("generated/test")
    }
    fn apply(&self, id: &str) -> chroma::Result<usize> {
        apply::apply(
            &self.paths,
            &self.config,
            id,
            presets::get(id).unwrap(),
            false,
        )
    }
}

#[test]
fn apply_backup_rollback_restores_contents_permissions_and_current() {
    let mut f = Fixture::new();
    fs::write(f.output(), b"original\0binary").unwrap();
    fs::set_permissions(f.output(), fs::Permissions::from_mode(0o640)).unwrap();
    f.config.adapters.get_mut("test").unwrap().allow_overwrite = true;
    f.apply("rustwood").unwrap();
    let first = fs::read(f.output()).unwrap();
    let state = State::load(&f.paths.state).unwrap();
    let tx = backup::load(&f.paths.state, state.head.as_ref().unwrap()).unwrap();
    assert_eq!(
        tx.entries[0].before.as_ref().unwrap().bytes,
        b"original\0binary"
    );
    assert_eq!(
        fs::metadata(f.output()).unwrap().permissions().mode() & 0o777,
        0o640
    );
    f.apply("nord").unwrap();
    assert_ne!(fs::read(f.output()).unwrap(), first);
    rollback::rollback(&f.paths, &f.config, true).unwrap();
    assert_eq!(fs::read(f.output()).unwrap(), first);
    assert_eq!(
        State::load(&f.paths.state).unwrap().current.unwrap().id,
        "rustwood"
    );
    rollback::rollback(&f.paths, &f.config, true).unwrap();
    assert_eq!(fs::read(f.output()).unwrap(), b"original\0binary");
    assert_eq!(
        fs::metadata(f.output()).unwrap().permissions().mode() & 0o777,
        0o640
    );
    assert!(State::load(&f.paths.state).unwrap().current.is_none());
    assert!(rollback::rollback(&f.paths, &f.config, true).is_err());
}

#[test]
fn newly_created_files_are_deleted_by_rollback() {
    let f = Fixture::new();
    f.apply("paper").unwrap();
    assert!(f.output().exists());
    rollback::rollback(&f.paths, &f.config, true).unwrap();
    assert!(!f.output().exists());
}

#[test]
fn unmanaged_files_and_external_edits_require_local_authorization() {
    let f = Fixture::new();
    fs::write(f.output(), b"keep me").unwrap();
    assert!(
        f.apply("nord")
            .unwrap_err()
            .to_string()
            .contains("unmanaged")
    );
    assert_eq!(fs::read(f.output()).unwrap(), b"keep me");
    fs::remove_file(f.output()).unwrap();
    f.apply("nord").unwrap();
    fs::write(f.output(), b"edited manually").unwrap();
    assert!(f.apply("paper").is_err());
    assert!(rollback::rollback(&f.paths, &f.config, true).is_err());
    assert_eq!(fs::read(f.output()).unwrap(), b"edited manually");
    assert!(State::load(&f.paths.state).unwrap().pending.is_some());
}

#[test]
fn missing_template_and_late_invalid_adapter_leave_all_outputs_unchanged() {
    let mut f = Fixture::new();
    let mut adapter = f.config.adapters["test"].clone();
    adapter.template = Some("templates/missing".into());
    adapter.output = Some("generated/second".into());
    f.config.adapters.insert("z-late".into(), adapter.clone());
    assert!(f.apply("nord").is_err());
    assert!(!f.output().exists());
    adapter.template = Some("templates/test".into());
    adapter.output = Some("missing-parent/output".into());
    f.config.adapters.insert("z-late".into(), adapter);
    assert!(f.apply("nord").is_err());
    assert!(!f.output().exists());
    assert!(State::load(&f.paths.state).unwrap().head.is_none());
}

#[test]
fn duplicate_outputs_core_paths_and_parent_traversal_fail() {
    let mut f = Fixture::new();
    f.config
        .adapters
        .insert("duplicate".into(), f.config.adapters["test"].clone());
    assert!(f.apply("nord").is_err());
    f.config.adapters.remove("duplicate");
    for bad in [
        "config.toml",
        "templates/test",
        "state/lock",
        "generated/../config.toml",
        "/",
    ] {
        f.config.adapters.get_mut("test").unwrap().output = Some(bad.into());
        assert!(f.apply("nord").is_err(), "{bad}");
    }
}

#[test]
fn leaf_ancestor_state_symlinks_and_hardlinks_are_refused() {
    let f = Fixture::new();
    let outside = f._root.path().join("outside");
    fs::write(&outside, b"untouched").unwrap();
    symlink(&outside, f.output()).unwrap();
    assert!(f.apply("nord").is_err());
    fs::remove_file(f.output()).unwrap();
    fs::hard_link(&outside, f.output()).unwrap();
    assert!(f.apply("nord").is_err());
    fs::remove_file(f.output()).unwrap();
    fs::remove_dir(f.paths.config.join("generated")).unwrap();
    symlink(f._root.path(), f.paths.config.join("generated")).unwrap();
    assert!(f.apply("nord").is_err());
    assert_eq!(fs::read(outside).unwrap(), b"untouched");
    let g = Fixture::new();
    symlink(g._root.path(), &g.paths.state).unwrap();
    assert!(g.apply("nord").is_err());
    assert!(!g.output().exists());
}

#[test]
fn writable_by_other_users_or_directory_targets_are_refused() {
    let f = Fixture::new();
    fs::create_dir(f.output()).unwrap();
    assert!(f.apply("nord").is_err());
    fs::remove_dir(f.output()).unwrap();
    fs::set_permissions(
        f.paths.config.join("generated"),
        fs::Permissions::from_mode(0o777),
    )
    .unwrap();
    assert!(f.apply("nord").is_err());
}

#[test]
fn reload_failure_reverts_all_files_and_previous_state() {
    let mut f = Fixture::new();
    f.apply("rustwood").unwrap();
    let old = fs::read(f.output()).unwrap();
    let head = State::load(&f.paths.state).unwrap().head;
    let mut second = f.config.adapters["test"].clone();
    second.output = Some("generated/second".into());
    second.reload = vec!["/usr/bin/false".into()];
    f.config.adapters.insert("second".into(), second);
    f.config.general.auto_reload = true;
    let error = f.apply("paper").unwrap_err();
    assert!(format!("{error:#}").contains("restored"));
    assert_eq!(fs::read(f.output()).unwrap(), old);
    assert!(!f.paths.config.join("generated/second").exists());
    let state = State::load(&f.paths.state).unwrap();
    assert_eq!(state.current.unwrap().id, "rustwood");
    assert_eq!(state.head, head);
    assert!(state.pending.is_none());
}

#[test]
fn interrupted_partial_apply_is_recoverable_without_running_commands() {
    let f = Fixture::new();
    f.apply("rustwood").unwrap();
    let mut state = State::load(&f.paths.state).unwrap();
    let before = Dir::parent(&f.output())
        .unwrap()
        .0
        .read(f.output().file_name().unwrap())
        .unwrap();
    let next = Snapshot {
        bytes: b"partially applied".to_vec(),
        mode: 0o600,
    };
    let tx = Transaction {
        parent: state.head.clone(),
        before: state.current.clone(),
        after: None,
        entries: vec![
            Entry {
                adapter: "test".into(),
                path: f.output(),
                before: before.clone(),
                after: next.clone(),
            },
            Entry {
                adapter: "test".into(),
                path: f.paths.config.join("generated/not-yet-written"),
                before: None,
                after: next.clone(),
            },
        ],
    };
    let id = backup::create(&f.paths.state, &tx).unwrap();
    state.pending = Some(Pending {
        transaction: id,
        restoring: false,
    });
    state.save(&f.paths.state).unwrap();
    chroma::safe_fs::write(&f.output(), &next.bytes, true).unwrap();
    assert!(f.apply("nord").is_err());
    rollback::rollback(&f.paths, &f.config, true).unwrap();
    assert_eq!(fs::read(f.output()).unwrap(), before.unwrap().bytes);
    assert!(!f.paths.config.join("generated/not-yet-written").exists());
    assert!(State::load(&f.paths.state).unwrap().pending.is_none());
}

#[test]
fn incomplete_rollback_keeps_backup_and_can_be_retried() {
    let f = Fixture::new();
    f.apply("rustwood").unwrap();
    let generated = fs::read(f.output()).unwrap();
    fs::write(f.output(), b"external edit").unwrap();
    assert!(rollback::rollback(&f.paths, &f.config, true).is_err());
    let state = State::load(&f.paths.state).unwrap();
    assert!(backup::load(&f.paths.state, &state.pending.unwrap().transaction).is_ok());
    fs::write(f.output(), generated).unwrap();
    rollback::rollback(&f.paths, &f.config, true).unwrap();
    assert!(!f.output().exists());
}

#[test]
fn state_lock_prevents_overlapping_operations() {
    let f = Fixture::new();
    let dir = Dir::open(&f.paths.state, true).unwrap();
    let _lock = dir.lock().unwrap();
    assert!(f.apply("nord").is_err());
    assert!(!f.output().exists());
}

#[test]
fn atomic_new_file_creation_does_not_overwrite_existing_content() {
    let f = Fixture::new();
    chroma::safe_fs::write(&f.output(), b"first", false).unwrap();
    assert!(chroma::safe_fs::write(&f.output(), b"second", false).is_err());
    assert_eq!(fs::read(f.output()).unwrap(), b"first");
    assert_eq!(
        fs::read_dir(f.paths.config.join("generated"))
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn config_path_is_diagnostic_and_never_replaced() {
    let mut f = Fixture::new();
    f.config.adapters.get_mut("test").unwrap().config_path = Some("generated/test".into());
    assert!(f.apply("nord").is_err());
    assert!(!f.output().exists());
}

#[test]
fn one_adapter_cannot_replace_another_adapters_input() {
    let mut f = Fixture::new();
    let other = Adapter {
        config_path: Some("generated/test".into()),
        ..Default::default()
    };
    f.config.adapters.insert("other".into(), other);
    assert!(f.apply("nord").is_err());
    assert!(!f.output().exists());
}

#[test]
fn read_only_destination_parent_fails_preflight() {
    let f = Fixture::new();
    fs::set_permissions(
        f.paths.config.join("generated"),
        fs::Permissions::from_mode(0o500),
    )
    .unwrap();
    let error = f.apply("nord").unwrap_err();
    assert!(format!("{error:#}").contains("write/execute"));
    assert!(!f.output().exists());
    assert!(!f.paths.state.join("transactions").exists());
}

#[test]
fn corrupt_state_does_not_touch_outputs() {
    let f = Fixture::new();
    Dir::open(&f.paths.state, true).unwrap();
    fs::write(f.paths.state.join("state.toml"), "not toml").unwrap();
    assert!(f.apply("nord").is_err());
    assert!(!f.output().exists());
}

#[test]
fn automatic_recovery_restores_a_simulated_partial_write_failure() {
    let f = Fixture::new();
    f.apply("rustwood").unwrap();
    let state = State::load(&f.paths.state).unwrap();
    let old = state.current.as_ref().unwrap().files[&f.output()].clone();
    let after = Snapshot {
        bytes: b"new".to_vec(),
        mode: old.mode,
    };
    let tx = Transaction {
        parent: state.head.clone(),
        before: state.current.clone(),
        after: None,
        entries: vec![Entry {
            adapter: "test".into(),
            path: f.output(),
            before: Some(old.clone()),
            after: after.clone(),
        }],
    };
    chroma::safe_fs::write(&f.output(), &after.bytes, true).unwrap();
    let error = engine::rollback::recover_failed::<()>(
        &f.paths,
        &f.config,
        &tx,
        false,
        anyhow::anyhow!("injected I/O failure"),
    )
    .unwrap_err();
    assert!(format!("{error:#}").contains("injected I/O failure"));
    assert_eq!(fs::read(f.output()).unwrap(), old.bytes);
}
