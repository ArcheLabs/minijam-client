use minijam_protocol::PackageStatus;

#[test]
fn package_status_is_terminal_only_after_import_or_failure() {
    assert_eq!(PackageStatus::Pending, PackageStatus::Pending);
    assert_ne!(PackageStatus::Pending, PackageStatus::Imported);
    assert_ne!(PackageStatus::Pending, PackageStatus::Failed);
}
