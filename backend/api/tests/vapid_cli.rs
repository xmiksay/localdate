//! `localdate-api vapid generate` prints a key pair the server accepts.

use localdate_api::config::VapidConfig;
use localdate_api::push::vapid::Vapid;

#[test]
fn vapid_generate_prints_a_usable_key_pair() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_localdate-api"))
        .args(["vapid", "generate"])
        .output()
        .expect("run localdate-api");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).expect("utf-8");
    let value = |name: &str| {
        stdout
            .lines()
            .find_map(|l| l.strip_prefix(&format!("{name}=")))
            .unwrap_or_else(|| panic!("{name} missing in {stdout}"))
            .to_owned()
    };
    let config = VapidConfig {
        public_key: value("VAPID_PUBLIC_KEY"),
        private_key: value("VAPID_PRIVATE_KEY"),
        subject: "mailto:ops@example.com".into(),
    };
    let vapid = Vapid::from_config(&config).expect("generated keys are accepted");
    assert_eq!(vapid.public_key, config.public_key);
}
