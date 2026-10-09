//! `PhotoStore`: the one place photo files are written, read and deleted — a local directory or an
//! S3-compatible bucket, both through `object_store`.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use bytes::Bytes;
use futures_util::StreamExt;
use futures_util::stream::{self, BoxStream};
use object_store::aws::AmazonS3Builder;
use object_store::local::LocalFileSystem;
use object_store::path::Path;
use object_store::{ClientOptions, ObjectMeta, ObjectStore, ObjectStoreExt, RetryConfig};

use crate::config::{PhotoStorage, S3Config};

/// No total request timeout: it would also cut off a slow but progressing `/media` stream; the
/// read timeout bounds each wait for headers or the next chunk instead.
const S3_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const S3_READ_TIMEOUT: Duration = Duration::from_secs(30);
/// The crate's default retries for minutes; an upload should fail within seconds instead.
const S3_MAX_RETRIES: usize = 2;
const S3_RETRY_TIMEOUT: Duration = Duration::from_secs(15);
/// Bounds the whole best-effort cleanup of one request (one bulk delete on S3).
const REMOVE_TIMEOUT: Duration = Duration::from_secs(20);
/// Startup probe objects; no photo name starts with it, so `/media` can never serve one.
const PROBE_PREFIX: &str = "_probe-";

/// Size and validator of a stored photo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Meta {
    pub size: u64,
    pub e_tag: Option<String>,
}

impl From<ObjectMeta> for Meta {
    fn from(m: ObjectMeta) -> Self {
        Self {
            size: m.size,
            e_tag: m.e_tag,
        }
    }
}

pub type BodyStream = BoxStream<'static, Result<Bytes, object_store::Error>>;

#[derive(Clone)]
pub struct PhotoStore {
    inner: Arc<dyn ObjectStore>,
}

impl PhotoStore {
    /// Disk creates its directory; nothing touches the network here (see [`Self::check`]).
    pub fn new(cfg: &PhotoStorage) -> Result<Self> {
        let inner: Arc<dyn ObjectStore> = match cfg {
            PhotoStorage::Disk { dir } => {
                std::fs::create_dir_all(dir)
                    .with_context(|| format!("creating PHOTO_DIR {}", dir.display()))?;
                // Writes go to a staging file renamed into place, so a reader never sees half a
                // photo; fsync makes a stored photo survive a node crash like an S3 PUT does.
                Arc::new(
                    LocalFileSystem::new_with_prefix(dir)
                        .with_context(|| format!("opening PHOTO_DIR {}", dir.display()))?
                        .with_fsync(true),
                )
            }
            PhotoStorage::S3(s3) => Arc::new(s3_store(s3)?),
        };
        Ok(Self { inner })
    }

    /// Writes and deletes a probe object: proves the bucket exists and the key may write and
    /// delete, which reading alone would not. Classify a failure with [`Self::is_transient`].
    pub async fn check(&self) -> Result<(), object_store::Error> {
        let probe = Path::from(format!("{PROBE_PREFIX}{}", uuid::Uuid::new_v4()));
        self.inner.put(&probe, b"probe".to_vec().into()).await?;
        self.inner.delete(&probe).await
    }

    /// Whether waiting could fix a [`Self::check`] failure: transport errors and 5xx can, while a
    /// refusal (403 credentials, 404 bucket, any other 4xx) or a bad local path will not.
    pub fn is_transient(err: &object_store::Error) -> bool {
        match err {
            object_store::Error::Generic { .. } => !client_error_status(err),
            _ => false,
        }
    }

    /// Atomic: readers see the whole object or none.
    pub async fn put(&self, name: &str, bytes: Vec<u8>) -> Result<()> {
        self.inner
            .put(&key(name)?, bytes.into())
            .await
            .with_context(|| format!("storing photo {name}"))?;
        Ok(())
    }

    /// `None` when missing.
    pub async fn head(&self, name: &str) -> Result<Option<Meta>> {
        match self.inner.head(&key(name)?).await {
            Ok(meta) => Ok(Some(meta.into())),
            Err(object_store::Error::NotFound { .. }) => Ok(None),
            Err(e) => Err(e).with_context(|| format!("looking up photo {name}")),
        }
    }

    /// The body as a stream; `None` when missing (decided before the first byte).
    pub async fn get(&self, name: &str) -> Result<Option<(Meta, BodyStream)>> {
        match self.inner.get(&key(name)?).await {
            Ok(got) => Ok(Some((got.meta.clone().into(), got.into_stream()))),
            Err(object_store::Error::NotFound { .. }) => Ok(None),
            Err(e) => Err(e).with_context(|| format!("reading photo {name}")),
        }
    }

    /// Best effort, one bulk delete within [`REMOVE_TIMEOUT`]: a leftover object is garbage, not
    /// a reason to fail the request. Missing objects count as deleted. Returns the names that
    /// could not be deleted, also logged in one line so they can be removed by hand (no row points
    /// at them any more).
    pub async fn remove_all(&self, names: &[String]) -> Vec<String> {
        if names.is_empty() {
            return Vec::new();
        }
        let paths: Vec<object_store::Result<Path>> = names
            .iter()
            .map(|n| Path::parse(n).map_err(object_store::Error::from))
            .collect();
        let mut results = self.inner.delete_stream(stream::iter(paths).boxed());
        let mut deleted = HashSet::new();
        let finished = tokio::time::timeout(REMOVE_TIMEOUT, async {
            while let Some(result) = results.next().await {
                match result {
                    Ok(path) => {
                        deleted.insert(path.to_string());
                    }
                    // The disk backend names the file by its full path here.
                    Err(object_store::Error::NotFound { path, .. }) => {
                        if let Some(n) = names.iter().find(|n| path.ends_with(n.as_str())) {
                            deleted.insert(n.clone());
                        }
                    }
                    Err(e) => tracing::warn!(error = %e, "deleting photo failed"),
                }
            }
        })
        .await;
        if finished.is_err() {
            tracing::warn!(timeout = ?REMOVE_TIMEOUT, "deleting photos timed out");
        }
        let leftover: Vec<String> = names
            .iter()
            .filter(|n| !deleted.contains(n.as_str()))
            .cloned()
            .collect();
        if !leftover.is_empty() {
            tracing::warn!(?leftover, "photo files left behind in storage");
        }
        leftover
    }

    #[cfg(test)]
    fn in_memory() -> Self {
        Self {
            inner: Arc::new(object_store::memory::InMemory::new()),
        }
    }
}

/// `object_store` keeps the HTTP status of a `Generic` error in a crate-private type, so this reads
/// it from the message ("… status code: 4xx …") instead.
fn client_error_status(err: &object_store::Error) -> bool {
    let mut source: Option<&(dyn std::error::Error + 'static)> = Some(err);
    while let Some(e) = source {
        if e.to_string().contains("status code: 4") {
            return true;
        }
        source = e.source();
    }
    false
}

fn key(name: &str) -> Result<Path> {
    Path::parse(name)
        .ok()
        .filter(|p| p.parts().next().is_some())
        .with_context(|| format!("invalid photo name {name:?}"))
}

fn s3_store(cfg: &S3Config) -> Result<object_store::aws::AmazonS3> {
    // reqwest 0.13 is built without a bundled crypto provider (no aws-lc-rs); it panics when it
    // builds a TLS client with none installed. Err only means one is installed already.
    let _ = rustls::crypto::ring::default_provider().install_default();
    AmazonS3Builder::new()
        .with_endpoint(&cfg.endpoint)
        .with_virtual_hosted_style_request(false)
        .with_region(&cfg.region)
        .with_bucket_name(&cfg.bucket)
        .with_access_key_id(&cfg.access_key_id)
        .with_secret_access_key(&cfg.secret_access_key)
        .with_client_options(
            // In-cluster Garage is plain http; `allow_http` lives here, not on the builder.
            ClientOptions::new()
                .with_allow_http(cfg.endpoint.starts_with("http://"))
                .with_timeout_disabled()
                .with_connect_timeout(S3_CONNECT_TIMEOUT)
                .with_read_timeout(S3_READ_TIMEOUT),
        )
        .with_retry(RetryConfig {
            backoff: Default::default(),
            max_retries: S3_MAX_RETRIES,
            retry_timeout: S3_RETRY_TIMEOUT,
        })
        .build()
        .context("configuring S3 photo storage")
}

#[cfg(test)]
mod tests {
    use futures_util::TryStreamExt;

    use super::*;

    #[tokio::test]
    async fn put_get_head_delete_round_trip() {
        let store = PhotoStore::in_memory();
        let name = "0b7c6a1e-9d4f-4a8e-8c3b-2f1e0d9c8b7a.webp";
        assert!(store.get(name).await.expect("get").is_none());
        assert!(store.head(name).await.expect("head").is_none());

        store.put(name, b"webp".to_vec()).await.expect("put");
        let (meta, body) = store.get(name).await.expect("get").expect("stored");
        assert_eq!(meta.size, 4);
        let chunks: Vec<Bytes> = body.try_collect().await.expect("body");
        assert_eq!(chunks.concat(), b"webp");
        assert_eq!(
            store.head(name).await.expect("head").map(|m| m.size),
            Some(4)
        );

        assert!(store.remove_all(&[name.to_owned()]).await.is_empty());
        assert!(store.get(name).await.expect("get").is_none());
        assert!(
            store.remove_all(&[name.to_owned()]).await.is_empty(),
            "missing = deleted"
        );
        store.check().await.expect("in-memory store answers");
    }

    #[test]
    fn names_that_escape_the_root_are_refused() {
        for bad in ["", "../x.webp", "a//b", "a/./b", "a\nb"] {
            assert!(key(bad).is_err(), "{bad:?}");
        }
    }

    #[tokio::test]
    async fn disk_store_creates_its_directory() {
        let tmp = tempfile::tempdir().expect("tmp");
        let dir = tmp.path().join("nested/photos");
        let store = PhotoStore::new(&PhotoStorage::Disk { dir: dir.clone() }).expect("disk");
        store.put("a.webp", b"x".to_vec()).await.expect("put");
        let names: Vec<_> = std::fs::read_dir(&dir)
            .expect("dir")
            .map(|e| e.expect("entry").file_name())
            .collect();
        assert_eq!(names, ["a.webp"], "no staging file left behind");

        store.check().await.expect("disk is writable");
        let left = store
            .remove_all(&["a.webp".to_owned(), "gone.webp".to_owned()])
            .await;
        assert!(left.is_empty(), "{left:?}");
        assert_eq!(
            std::fs::read_dir(&dir).expect("dir").count(),
            0,
            "probe removed too"
        );
    }

    #[test]
    fn only_outages_are_transient() {
        let generic = |msg: &str| object_store::Error::Generic {
            store: "S3",
            source: msg.to_owned().into(),
        };
        assert!(PhotoStore::is_transient(&generic(
            "error sending request: connection refused"
        )));
        assert!(PhotoStore::is_transient(&generic(
            "Server returned non-2xx status code: 503 Service Unavailable"
        )));
        assert!(!PhotoStore::is_transient(&generic(
            "Server returned non-2xx status code: 400 Bad Request: AuthorizationHeaderMalformed"
        )));
        for permanent in [
            object_store::Error::PermissionDenied {
                path: "p".into(),
                source: "403".into(),
            },
            object_store::Error::NotFound {
                path: "p".into(),
                source: "NoSuchBucket".into(),
            },
        ] {
            assert!(!PhotoStore::is_transient(&permanent), "{permanent}");
        }
    }
}
