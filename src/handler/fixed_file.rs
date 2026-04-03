use std::fs;
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;

use crate::common::utils::{is_header_name_valid, validate_header_value};
use crate::common::{Handler, Method, Request, Response};
use crate::error::Error;

use super::core::Route;

const RESERVED_EXTRA_HEADERS: [&str; 4] = [
    "Content-Type",
    "Cache-Control",
    "Content-Disposition",
    "Content-Length",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixedFileOptions {
    pub content_type: Option<String>,
    pub cache_control: Option<String>,
    pub content_disposition: Option<String>,
    pub extra_headers: Vec<(String, String)>,
    pub register_head: bool,
}

impl Default for FixedFileOptions {
    fn default() -> Self {
        Self {
            content_type: None,
            cache_control: None,
            content_disposition: None,
            extra_headers: Vec::new(),
            register_head: true,
        }
    }
}

impl FixedFileOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn content_type(mut self, value: impl Into<String>) -> Self {
        self.content_type = Some(value.into());
        self
    }

    pub fn cache_control(mut self, value: impl Into<String>) -> Self {
        self.cache_control = Some(value.into());
        self
    }

    pub fn content_disposition(mut self, value: impl Into<String>) -> Self {
        self.content_disposition = Some(value.into());
        self
    }

    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra_headers.push((name.into(), value.into()));
        self
    }

    pub fn without_head(mut self) -> Self {
        self.register_head = false;
        self
    }
}

#[derive(Debug, Clone)]
pub(crate) struct FixedFileAsset {
    pub(crate) body: Bytes,
    pub(crate) content_type: String,
    pub(crate) cache_control: Option<String>,
    pub(crate) content_disposition: Option<String>,
    pub(crate) extra_headers: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub(crate) struct LoadedFixedFile {
    pub(crate) asset: FixedFileAsset,
    pub(crate) register_head: bool,
}

#[derive(Clone)]
pub(crate) struct FixedFileHandler {
    asset: Arc<FixedFileAsset>,
}

impl FixedFileHandler {
    pub(crate) fn new(asset: Arc<FixedFileAsset>) -> Self {
        Self { asset }
    }
}

#[async_trait]
impl Handler for FixedFileHandler {
    async fn handle(&self, _req: Request) -> Result<Response, Error> {
        let mut response = Response::ok()
            .with_header("Content-Type", self.asset.content_type.clone())
            .with_body(self.asset.body.clone());

        if let Some(cache_control) = &self.asset.cache_control {
            response = response.with_header("Cache-Control", cache_control.clone());
        }

        if let Some(content_disposition) = &self.asset.content_disposition {
            response = response.with_header("Content-Disposition", content_disposition.clone());
        }

        for (name, value) in &self.asset.extra_headers {
            response = response.append_header(name.clone(), value.clone());
        }

        Ok(response)
    }
}

#[derive(Clone)]
struct ValidationHandler;

#[async_trait]
impl Handler for ValidationHandler {
    async fn handle(&self, _req: Request) -> Result<Response, Error> {
        Ok(Response::ok())
    }
}

pub(crate) fn validate_fixed_file_route_path(route_path: &str) -> Result<(), Error> {
    if route_path.contains('{') || route_path.contains('}') {
        return Err(Error::ConfigurationError(format!(
            "fixed file route must be a static path without parameters: {}",
            route_path
        )));
    }

    Route::new(Method::GET, route_path.to_string(), ValidationHandler).map(|_| ())
}

pub(crate) fn validate_fixed_file_options(options: &FixedFileOptions) -> Result<(), Error> {
    validate_optional_header_value("Content-Type", options.content_type.as_deref())?;
    validate_optional_header_value("Cache-Control", options.cache_control.as_deref())?;
    validate_optional_header_value(
        "Content-Disposition",
        options.content_disposition.as_deref(),
    )?;

    for (name, value) in &options.extra_headers {
        if !is_header_name_valid(name) {
            return Err(Error::InvalidHeader(format!(
                "invalid fixed file header name: {}",
                name
            )));
        }

        validate_header_value(value)?;

        if RESERVED_EXTRA_HEADERS
            .iter()
            .any(|reserved| reserved.eq_ignore_ascii_case(name))
        {
            return Err(Error::ConfigurationError(format!(
                "fixed file extra_headers must not override reserved header: {}",
                name
            )));
        }
    }

    Ok(())
}

pub(crate) fn load_fixed_file(
    file_path: &Path,
    options: FixedFileOptions,
) -> Result<LoadedFixedFile, Error> {
    let metadata = fs::metadata(file_path).map_err(|err| {
        Error::ConfigurationError(format!(
            "failed to read fixed file metadata '{}': {}",
            file_path.display(),
            err
        ))
    })?;

    if !metadata.is_file() {
        return Err(Error::ConfigurationError(format!(
            "fixed file path must point to a regular file: {}",
            file_path.display()
        )));
    }

    let body = fs::read(file_path).map_err(|err| {
        Error::ConfigurationError(format!(
            "failed to read fixed file '{}': {}",
            file_path.display(),
            err
        ))
    })?;

    let content_type = options
        .content_type
        .unwrap_or_else(|| detect_content_type(file_path));

    Ok(LoadedFixedFile {
        asset: FixedFileAsset {
            body: Bytes::from(body),
            content_type,
            cache_control: options.cache_control,
            content_disposition: options.content_disposition,
            extra_headers: options.extra_headers,
        },
        register_head: options.register_head,
    })
}

pub(crate) fn detect_content_type(file_path: &Path) -> String {
    let extension = file_path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase());

    match extension.as_deref() {
        Some("txt") => "text/plain; charset=utf-8".to_string(),
        Some("html") => "text/html; charset=utf-8".to_string(),
        Some("css") => "text/css; charset=utf-8".to_string(),
        Some("js") => "text/javascript; charset=utf-8".to_string(),
        Some("json") => "application/json".to_string(),
        Some("svg") => "image/svg+xml".to_string(),
        _ => mime_guess::from_path(file_path)
            .first_raw()
            .unwrap_or("application/octet-stream")
            .to_string(),
    }
}

fn validate_optional_header_value(header_name: &str, value: Option<&str>) -> Result<(), Error> {
    if let Some(value) = value {
        validate_header_value(value).map_err(|_| {
            Error::InvalidHeader(format!(
                "invalid fixed file {} value: contains control characters",
                header_name
            ))
        })?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RunBridge;
    use std::path::PathBuf;
    use std::process;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST_ID: AtomicU64 = AtomicU64::new(0);

    fn unique_path(name: &str) -> PathBuf {
        let id = NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "runbridge_fixed_file_{}_{}_{}",
            process::id(),
            id,
            name
        ))
    }

    fn write_test_file(name: &str, body: &[u8]) -> PathBuf {
        let path = unique_path(name);
        fs::write(&path, body).expect("test file must be written");
        path
    }

    #[test]
    fn detect_content_type_prefers_text_defaults() {
        assert_eq!(
            detect_content_type(Path::new("robots.txt")),
            "text/plain; charset=utf-8"
        );
        assert_eq!(
            detect_content_type(Path::new("site.html")),
            "text/html; charset=utf-8"
        );
        assert_eq!(
            detect_content_type(Path::new("app.css")),
            "text/css; charset=utf-8"
        );
        assert_eq!(
            detect_content_type(Path::new("app.js")),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(
            detect_content_type(Path::new("data.json")),
            "application/json"
        );
        assert_eq!(detect_content_type(Path::new("logo.svg")), "image/svg+xml");
    }

    #[test]
    fn load_fixed_file_reads_existing_file() {
        let path = write_test_file("hello.txt", b"hello fixed file");
        let loaded = load_fixed_file(&path, FixedFileOptions::new()).expect("file must load");

        assert_eq!(loaded.asset.body, Bytes::from_static(b"hello fixed file"));
        assert_eq!(loaded.asset.content_type, "text/plain; charset=utf-8");
        assert!(loaded.register_head);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn load_fixed_file_rejects_missing_file() {
        let path = unique_path("missing.txt");
        let err = load_fixed_file(&path, FixedFileOptions::new()).expect_err("must fail");

        assert!(matches!(err, Error::ConfigurationError(_)));
    }

    #[test]
    fn load_fixed_file_rejects_directory() {
        let path = unique_path("dir");
        fs::create_dir(&path).expect("test directory must be created");

        let err = load_fixed_file(&path, FixedFileOptions::new()).expect_err("must fail");
        assert!(matches!(err, Error::ConfigurationError(_)));

        let _ = fs::remove_dir(&path);
    }

    #[cfg(unix)]
    #[test]
    fn load_fixed_file_rejects_unreadable_file() {
        use std::os::unix::fs::PermissionsExt;

        let path = write_test_file("private.txt", b"secret");
        let mut permissions = fs::metadata(&path)
            .expect("metadata must exist")
            .permissions();
        permissions.set_mode(0o000);
        fs::set_permissions(&path, permissions).expect("permissions must be updated");

        let err = load_fixed_file(&path, FixedFileOptions::new()).expect_err("must fail");
        assert!(matches!(err, Error::ConfigurationError(_)));

        let mut restore = fs::metadata(&path)
            .expect("metadata must exist")
            .permissions();
        restore.set_mode(0o600);
        let _ = fs::set_permissions(&path, restore);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn validate_fixed_file_route_path_rejects_path_params() {
        let err = validate_fixed_file_route_path("/files/{name}").expect_err("must fail");
        assert!(matches!(err, Error::ConfigurationError(_)));
    }

    #[test]
    fn validate_fixed_file_options_rejects_reserved_headers() {
        let err = validate_fixed_file_options(
            &FixedFileOptions::new().header("Content-Type", "application/json"),
        )
        .expect_err("must fail");

        assert!(matches!(err, Error::ConfigurationError(_)));
    }

    #[test]
    fn validate_fixed_file_options_rejects_invalid_header_name() {
        let err =
            validate_fixed_file_options(&FixedFileOptions::new().header("Bad Header", "value"))
                .expect_err("must fail");

        assert!(matches!(err, Error::InvalidHeader(_)));
    }

    #[test]
    fn builder_rejects_parameterized_route_path() {
        let path = write_test_file("route-check.txt", b"ok");

        let err = match RunBridge::builder().try_fixed_file("/files/{name}", &path) {
            Ok(_) => panic!("must fail"),
            Err(err) => err,
        };

        assert!(matches!(err, Error::ConfigurationError(_)));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn builder_rejects_reserved_extra_header_overrides() {
        let path = write_test_file("reserved-header.txt", b"ok");

        let err = match RunBridge::builder().try_fixed_file_with(
            "/reserved",
            &path,
            FixedFileOptions::new().header("Content-Length", "2"),
        ) {
            Ok(_) => panic!("must fail"),
            Err(err) => err,
        };

        assert!(matches!(err, Error::ConfigurationError(_)));

        let _ = fs::remove_file(path);
    }

    #[tokio::test]
    async fn fixed_file_handler_applies_configured_headers() {
        let handler = FixedFileHandler::new(Arc::new(FixedFileAsset {
            body: Bytes::from_static(b"\x00\xff"),
            content_type: "application/octet-stream".to_string(),
            cache_control: Some("public, max-age=60".to_string()),
            content_disposition: Some("inline".to_string()),
            extra_headers: vec![("X-Fixed".to_string(), "yes".to_string())],
        }));

        let response = handler
            .handle(Request::new(Method::GET, "/fixed.bin".to_string()))
            .await
            .expect("handler must succeed");

        assert_eq!(response.status, 200);
        assert_eq!(
            response.headers.get("Content-Type"),
            Some("application/octet-stream")
        );
        assert_eq!(
            response.headers.get("Cache-Control"),
            Some("public, max-age=60")
        );
        assert_eq!(response.headers.get("Content-Disposition"), Some("inline"));
        assert_eq!(response.headers.get("X-Fixed"), Some("yes"));
        assert_eq!(response.body, Some(Bytes::from_static(b"\x00\xff")));
    }
}
