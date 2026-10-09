//! `PHOTO_STORAGE` (`disk` | `s3`): where photo files live. `disk` uses `PHOTO_DIR`; `s3` uses the
//! `S3_*` variables exactly as Garage's `make bucket` Secret names them (the deployment `envFrom`s it).

use std::path::PathBuf;

use anyhow::{Context, Result, bail};

#[derive(Debug, Clone)]
pub enum PhotoStorage {
    Disk { dir: PathBuf },
    S3(S3Config),
}

/// Always addressed path-style (`{endpoint}/{bucket}/{key}`): Garage serves no bucket subdomains.
#[derive(Clone)]
pub struct S3Config {
    /// `http(s)://host[:port]`, no trailing slash.
    pub endpoint: String,
    pub region: String,
    pub bucket: String,
    pub access_key_id: String,
    pub secret_access_key: String,
}

/// `Config` is `Debug`; the credentials must never reach a log line through it.
impl std::fmt::Debug for S3Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("S3Config")
            .field("endpoint", &self.endpoint)
            .field("region", &self.region)
            .field("bucket", &self.bucket)
            .field("access_key_id", &"<redacted>")
            .field("secret_access_key", &"<redacted>")
            .finish()
    }
}

/// Reads the variables through `get` (the process env in `Config::from_env`). Empty counts as
/// unset, as an optional k8s Secret key leaves it.
pub(super) fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<PhotoStorage> {
    let get = |name: &str| {
        get(name)
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
    };
    match get("PHOTO_STORAGE").as_deref().unwrap_or("disk") {
        "disk" => Ok(PhotoStorage::Disk {
            dir: get("PHOTO_DIR")
                .context("PHOTO_DIR is required with PHOTO_STORAGE=disk")?
                .into(),
        }),
        "s3" => {
            let need = |name: &str| {
                get(name).with_context(|| format!("{name} is required with PHOTO_STORAGE=s3"))
            };
            let endpoint = need("S3_ENDPOINT")?;
            if !(endpoint.starts_with("http://") || endpoint.starts_with("https://")) {
                bail!("S3_ENDPOINT must start with http:// or https://");
            }
            Ok(PhotoStorage::S3(S3Config {
                endpoint: endpoint.trim_end_matches('/').to_owned(),
                region: need("S3_REGION")?,
                bucket: need("S3_BUCKET")?,
                access_key_id: need("S3_ACCESS_KEY_ID")?,
                secret_access_key: need("S3_SECRET_ACCESS_KEY")?,
            }))
        }
        other => bail!("PHOTO_STORAGE must be disk or s3, not {other:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S3: [(&str, &str); 6] = [
        ("PHOTO_STORAGE", "s3"),
        ("S3_ENDPOINT", "http://garage.services.svc:3900/"),
        ("S3_REGION", "garage"),
        ("S3_BUCKET", "localdate"),
        ("S3_ACCESS_KEY_ID", "GKkey-id"),
        ("S3_SECRET_ACCESS_KEY", "very-secret"),
    ];

    fn parse(pairs: &[(&str, &str)]) -> Result<PhotoStorage> {
        from_lookup(|name| {
            pairs
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| (*v).to_owned())
        })
    }

    #[test]
    fn disk_is_the_default_and_needs_photo_dir() {
        let got = parse(&[("PHOTO_DIR", "/data/photos")]).expect("disk");
        assert!(
            matches!(got, PhotoStorage::Disk { dir } if dir == std::path::Path::new("/data/photos"))
        );
        let got = parse(&[("PHOTO_STORAGE", " "), ("PHOTO_DIR", "x")]).expect("empty = default");
        assert!(matches!(got, PhotoStorage::Disk { .. }));
        let err = parse(&[("PHOTO_STORAGE", "disk")]).expect_err("no dir");
        assert!(err.to_string().contains("PHOTO_DIR"), "{err}");
    }

    #[test]
    fn s3_reads_every_variable_and_ignores_photo_dir() {
        let mut pairs = S3.to_vec();
        pairs.push(("S3_PUBLIC_ENDPOINT", "https://s3.mmik.cz"));
        let PhotoStorage::S3(s3) = parse(&pairs).expect("s3") else {
            panic!("expected s3");
        };
        assert_eq!(s3.endpoint, "http://garage.services.svc:3900");
        assert_eq!(
            (s3.region.as_str(), s3.bucket.as_str()),
            ("garage", "localdate")
        );
        assert_eq!(s3.access_key_id, "GKkey-id");
        assert_eq!(s3.secret_access_key, "very-secret");
    }

    #[test]
    fn s3_debug_redacts_the_credentials() {
        let PhotoStorage::S3(s3) = parse(&S3).expect("s3") else {
            panic!("expected s3");
        };
        let shown = format!("{s3:?}");
        assert!(shown.contains("garage.services.svc") && shown.contains("<redacted>"));
        assert!(
            !shown.contains("very-secret") && !shown.contains("GKkey-id"),
            "{shown}"
        );
    }

    #[test]
    fn s3_needs_every_variable() {
        for missing in [
            "S3_ENDPOINT",
            "S3_REGION",
            "S3_BUCKET",
            "S3_ACCESS_KEY_ID",
            "S3_SECRET_ACCESS_KEY",
        ] {
            let pairs: Vec<_> = S3.iter().copied().filter(|(k, _)| *k != missing).collect();
            let err = parse(&pairs).expect_err(missing);
            assert!(err.to_string().contains(missing), "{err}");
            let mut blank = S3.to_vec();
            blank.retain(|(k, _)| *k != missing);
            blank.push((missing, "  "));
            assert!(parse(&blank).is_err(), "{missing} blank");
        }
    }

    #[test]
    fn bad_kind_and_endpoint_scheme_are_refused() {
        let err = parse(&[("PHOTO_STORAGE", "gcs")]).expect_err("kind");
        assert!(err.to_string().contains("PHOTO_STORAGE"), "{err}");
        let mut pairs = S3.to_vec();
        pairs[1] = ("S3_ENDPOINT", "garage:3900");
        let err = parse(&pairs).expect_err("scheme");
        assert!(err.to_string().contains("S3_ENDPOINT"), "{err}");
    }
}
